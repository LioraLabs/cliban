//! Integration tests for the tenant manager: routing, isolation, caps,
//! concurrency, delete/export. File-backed DBs under a per-test temp dir
//! (OS temp cleanup reaps them; no tempfile dependency).

use std::path::PathBuf;
use std::sync::Arc;

use cliban_core::contexts::projects;
use cliban_core::Store;
use cliban_tenancy::{Caps, Error, TenantManager};

fn temp_data_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "cliban-tenancy-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn manager(name: &str) -> TenantManager {
    TenantManager::open(temp_data_dir(name), Caps::default()).unwrap()
}

async fn create_project(store: &Store, key: &str) {
    let key = key.to_string();
    store
        .call(move |c| {
            projects::create(
                c,
                projects::CreateProject {
                    key,
                    name: "Test".into(),
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap();
}

#[test]
fn open_creates_layout_and_routes_paths() {
    let dir = temp_data_dir("layout");
    let m = TenantManager::open(&dir, Caps::default()).unwrap();
    assert!(dir.join("registry.db").exists());
    assert!(dir.join("tenants").is_dir());
    assert_eq!(m.db_path("abc123"), dir.join("tenants").join("abc123.db"));
}

#[test]
fn handle_requires_registered_tenant() {
    let m = manager("unknown");
    assert!(matches!(
        m.handle("no-such-tenant"),
        Err(Error::TenantNotFound)
    ));
}

#[test]
fn handles_are_cached_and_share_one_write_lock() {
    let m = manager("cache");
    let owner = m.registry().create_user("alice").unwrap();
    let t = m.create_tenant(owner.id, "team").unwrap();

    let h1 = m.handle(&t.id).unwrap();
    let h2 = m.handle(&t.id).unwrap();
    assert!(Arc::ptr_eq(&h1.write_lock, &h2.write_lock));
}

#[test]
fn manager_enforces_caps() {
    let dir = temp_data_dir("caps");
    let caps = Caps {
        max_tenants_per_user: 2,
        max_tenants_global: 3,
    };
    let m = TenantManager::open(&dir, caps).unwrap();
    let alice = m.registry().create_user("alice").unwrap();
    let bob = m.registry().create_user("bob").unwrap();
    let carol = m.registry().create_user("carol").unwrap();

    m.create_tenant(alice.id, "a1").unwrap();
    m.create_tenant(alice.id, "a2").unwrap();
    assert!(matches!(
        m.create_tenant(alice.id, "a3"),
        Err(Error::CapExceeded(_))
    ));

    m.create_tenant(bob.id, "b1").unwrap();
    assert!(matches!(
        m.create_tenant(carol.id, "c1"),
        Err(Error::CapExceeded(_))
    ));
}

#[tokio::test]
async fn tenant_db_is_created_on_demand_with_core_schema() {
    let m = manager("on-demand");
    let owner = m.registry().create_user("alice").unwrap();
    let t = m.create_tenant(owner.id, "team").unwrap();

    // create_tenant is registry-only; the file appears on first handle().
    assert!(!m.db_path(&t.id).exists());
    let h = m.handle(&t.id).unwrap();
    assert!(m.db_path(&t.id).exists());

    // The DB speaks the standard cliban-core schema.
    create_project(&h.store, "OD").await;
    let listed = h.store.call(projects::list).await.unwrap();
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn tenants_cannot_see_each_others_data() {
    let m = manager("isolation");
    let owner = m.registry().create_user("alice").unwrap();
    let a = m.create_tenant(owner.id, "team-a").unwrap();
    let b = m.create_tenant(owner.id, "team-b").unwrap();

    let ha = m.handle(&a.id).unwrap();
    let hb = m.handle(&b.id).unwrap();
    create_project(&ha.store, "AA").await;

    // Tenant A sees its project; tenant B sees an empty database.
    let a_projects = ha.store.call(projects::list).await.unwrap();
    let b_projects = hb.store.call(projects::list).await.unwrap();
    assert_eq!(a_projects.len(), 1);
    assert_eq!(a_projects[0].key, "AA");
    assert!(b_projects.is_empty());

    // Isolation is physical: two distinct files on disk.
    assert_ne!(m.db_path(&a.id), m.db_path(&b.id));
    assert!(m.db_path(&a.id).exists());
    assert!(m.db_path(&b.id).exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_writes_to_one_tenant_serialize() {
    let m = manager("concurrency");
    let owner = m.registry().create_user("alice").unwrap();
    let t = m.create_tenant(owner.id, "team").unwrap();
    let h = m.handle(&t.id).unwrap();
    create_project(&h.store, "CC").await;

    // Each task performs a read-modify-write split across two store calls.
    // Without the per-tenant write lock the reads interleave and updates are
    // lost; with it, the final count is exact.
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let h = h.clone();
        tasks.push(tokio::spawn(async move {
            let _guard = h.lock_writes().await;
            let seq: i64 = h
                .store
                .call(|c| {
                    Ok(
                        c.query_row("SELECT issue_seq FROM projects WHERE key = 'CC'", [], |r| {
                            r.get(0)
                        })?,
                    )
                })
                .await
                .unwrap();
            h.store
                .call(move |c| {
                    c.execute(
                        "UPDATE projects SET issue_seq = ?1 WHERE key = 'CC'",
                        rusqlite::params![seq + 1],
                    )?;
                    Ok(())
                })
                .await
                .unwrap();
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }

    let seq: i64 = h
        .store
        .call(|c| {
            Ok(
                c.query_row("SELECT issue_seq FROM projects WHERE key = 'CC'", [], |r| {
                    r.get(0)
                })?,
            )
        })
        .await
        .unwrap();
    assert_eq!(seq, 20);
}

#[tokio::test]
async fn delete_tenant_removes_registry_rows_and_files() {
    let m = manager("delete");
    let owner = m.registry().create_user("alice").unwrap();
    let t = m.create_tenant(owner.id, "team").unwrap();
    let h = m.handle(&t.id).unwrap();
    create_project(&h.store, "DD").await;
    drop(h);

    m.delete_tenant(&t.id).unwrap();

    assert!(!m.db_path(&t.id).exists());
    assert!(m.registry().tenant(&t.id).unwrap().is_none());
    assert!(matches!(m.handle(&t.id), Err(Error::TenantNotFound)));
    // Second delete: the tenant is gone.
    assert!(matches!(m.delete_tenant(&t.id), Err(Error::TenantNotFound)));
}

#[tokio::test]
async fn export_tenant_produces_a_standalone_readable_copy() {
    let m = manager("export");
    let owner = m.registry().create_user("alice").unwrap();
    let t = m.create_tenant(owner.id, "team").unwrap();
    let h = m.handle(&t.id).unwrap();
    create_project(&h.store, "EE").await;

    let dest = temp_data_dir("export-out").join("export.db");
    m.export_tenant(&t.id, &dest).await.unwrap();

    // The copy opens as a normal cliban-core store and contains the data
    // (WAL was checkpointed into the main file before the copy).
    let copy = Store::open(&dest).unwrap();
    let listed = copy.call(projects::list).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].key, "EE");
}

#[tokio::test]
async fn export_unknown_tenant_fails() {
    let m = manager("export-unknown");
    let dest = temp_data_dir("export-unknown-out").join("export.db");
    assert!(matches!(
        m.export_tenant("nope", &dest).await,
        Err(Error::TenantNotFound)
    ));
}

#[test]
fn cached_handles_share_one_change_channel_scoped_per_tenant() {
    let m = manager("changes");
    let owner = m.registry().create_user("alice").unwrap();
    let a = m.create_tenant(owner.id, "team-a").unwrap();
    let b = m.create_tenant(owner.id, "team-b").unwrap();

    let ha1 = m.handle(&a.id).unwrap();
    let ha2 = m.handle(&a.id).unwrap();
    let hb = m.handle(&b.id).unwrap();

    let mut sub_same = ha2.changes.subscribe();
    let mut sub_other = hb.changes.subscribe();
    ha1.changes.send(()).unwrap();

    // Cloned handles for one tenant share the channel...
    assert!(sub_same.try_recv().is_ok());
    // ...and other tenants' subscribers hear nothing.
    assert!(sub_other.try_recv().is_err());
}
