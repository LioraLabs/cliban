//! Tenant manager: opens/creates per-tenant DBs on demand, caches handles,
//! enforces caps, and provides delete/export as file operations.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cliban_core::Store;
use tokio::sync::{broadcast, Mutex as AsyncMutex, OwnedMutexGuard};

use crate::error::{Error, Result};
use crate::registry::{Registry, Tenant};

/// Tenant-count caps enforced at creation time.
#[derive(Debug, Clone, Copy)]
pub struct Caps {
    /// Max tenants a single user may own (spec: "tenants-per-key", enforced
    /// per user since keys resolve to users).
    pub max_tenants_per_user: i64,
    /// Max tenants across the whole instance.
    pub max_tenants_global: i64,
}

impl Default for Caps {
    fn default() -> Caps {
        Caps {
            max_tenants_per_user: 5,
            max_tenants_global: 500,
        }
    }
}

/// Cached, cloneable handle to one tenant's DB.
#[derive(Clone)]
pub struct TenantHandle {
    /// The tenant's cliban-core store (own writer thread, WAL, migrated).
    pub store: Store,
    /// Serializes multi-call write sequences to this tenant. Public so
    /// callers (and tests) can see two handles share one lock.
    pub write_lock: Arc<AsyncMutex<()>>,
    /// Coarse per-tenant change feed. Writers send one `()` after each
    /// committed write; subscribers re-query on receipt. All clones of a
    /// tenant's handle share the sender, so every session on the tenant
    /// publishes into (and can subscribe to) the same feed. Small capacity
    /// is deliberate: a lagged receiver just coalesces missed events into
    /// one refresh.
    pub changes: broadcast::Sender<()>,
}

/// Routes tenant ids to per-tenant stores and owns the registry.
pub struct TenantManager {
    tenants_dir: PathBuf,
    registry: Registry,
    caps: Caps,
    handles: Mutex<HashMap<String, TenantHandle>>,
}

impl TenantHandle {
    /// Acquire this tenant's write lock. The Store already serializes each
    /// individual `call` on its writer thread; hold this guard across
    /// *multi-call* read-modify-write sequences so they don't interleave
    /// with other writers of the same tenant. By convention every writer
    /// that spans more than one `store.call` takes this lock first.
    pub async fn lock_writes(&self) -> OwnedMutexGuard<()> {
        self.write_lock.clone().lock_owned().await
    }
}

impl TenantManager {
    /// Open (or create) the data layout at `data_dir`:
    /// `<data_dir>/registry.db` and `<data_dir>/tenants/`.
    pub fn open(data_dir: impl AsRef<Path>, caps: Caps) -> Result<TenantManager> {
        let data_dir = data_dir.as_ref();
        let tenants_dir = data_dir.join("tenants");
        std::fs::create_dir_all(&tenants_dir)?;
        let registry = Registry::open(data_dir.join("registry.db"))?;
        Ok(TenantManager {
            tenants_dir,
            registry,
            caps,
            handles: Mutex::new(HashMap::new()),
        })
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Where a tenant's DB file lives (whether or not it exists yet).
    pub fn db_path(&self, tenant_id: &str) -> PathBuf {
        self.tenants_dir.join(format!("{tenant_id}.db"))
    }

    /// Create a tenant owned by `owner_user_id`, enforcing this manager's
    /// caps. Registry-only: the DB file is created lazily by [`handle`].
    ///
    /// [`handle`]: TenantManager::handle
    pub fn create_tenant(&self, owner_user_id: i64, slug: &str) -> Result<Tenant> {
        self.registry.create_tenant(
            slug,
            owner_user_id,
            self.caps.max_tenants_per_user,
            self.caps.max_tenants_global,
        )
    }

    /// Open-or-create the DB for a registered tenant and return its handle.
    /// Handles are cached: every caller for the same tenant shares one Store
    /// (one writer thread, WAL) and one write lock. First open runs the
    /// standard cliban-core migrations.
    pub fn handle(&self, tenant_id: &str) -> Result<TenantHandle> {
        // The registry check happens under the handles lock (same order as
        // `delete_tenant`: handles, then registry) so a concurrent delete
        // can't slip between the check and the open/insert and have the
        // deleted tenant's DB silently recreated with an unevictable cache
        // entry.
        let mut handles = self.handles.lock().expect("handles mutex poisoned");
        if self.registry.tenant(tenant_id)?.is_none() {
            return Err(Error::TenantNotFound);
        }
        if let Some(h) = handles.get(tenant_id) {
            return Ok(h.clone());
        }
        let store = Store::open(self.db_path(tenant_id))?;
        let (changes, _) = broadcast::channel(16);
        let h = TenantHandle {
            store,
            write_lock: Arc::new(AsyncMutex::new(())),
            changes,
        };
        handles.insert(tenant_id.to_string(), h.clone());
        Ok(h)
    }

    /// Delete a tenant: drop the cached handle, remove the registry rows
    /// (memberships and invites cascade), and delete the DB files
    /// (`.db`, `.db-wal`, `.db-shm`). Callers still holding a cloned handle
    /// keep the old writer thread alive until they drop it, but the files
    /// are gone and the tenant can no longer be routed to.
    pub fn delete_tenant(&self, tenant_id: &str) -> Result<()> {
        // Held for the whole delete (lock order: handles, then registry —
        // matching `handle`) so no concurrent `handle()` can re-open the DB
        // between the registry check and the file removal below.
        let mut handles = self.handles.lock().expect("handles mutex poisoned");
        if self.registry.tenant(tenant_id)?.is_none() {
            return Err(Error::TenantNotFound);
        }
        handles.remove(tenant_id);
        self.registry.delete_tenant(tenant_id)?;

        let db = self.db_path(tenant_id);
        let wal = db.with_extension("db-wal");
        let shm = db.with_extension("db-shm");
        for path in [db, wal, shm] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                // Never opened, or WAL already checkpointed away: fine.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// Export a tenant's DB to `dest` as a single standalone SQLite file:
    /// checkpoint the WAL into the main file, then copy it. Holds the
    /// tenant's write lock so no cooperating writer lands between the
    /// checkpoint and the copy.
    pub async fn export_tenant(&self, tenant_id: &str, dest: &Path) -> Result<()> {
        let handle = self.handle(tenant_id)?;
        let _guard = handle.lock_writes().await;
        handle
            .store
            .call(|conn| {
                // wal_checkpoint returns a result row; query_row consumes it.
                conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
                Ok(())
            })
            .await?;
        std::fs::copy(self.db_path(tenant_id), dest)?;
        Ok(())
    }
}
