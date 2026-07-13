//! Cross-tenant registry over `registry.db`: tenants, users, pubkeys,
//! memberships (owner|member), one-time invites.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Error, Result};
use cliban_core::time;

/// Cross-tenant registry handle. `Connection` is `Send + !Sync`; the mutex
/// makes the registry `Send + Sync`. Registry operations are single-row
/// lookups and tiny transactions, so blocking on this lock is fine.
pub struct Registry {
    conn: Mutex<Connection>,
}

/// Registry DDL. Everything is IF NOT EXISTS so reopening an existing
/// registry.db is a no-op; no migration ledger until a second schema
/// version actually exists.
const REGISTRY_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS "tenants" (
    "id" TEXT PRIMARY KEY,
    "slug" TEXT NOT NULL UNIQUE,
    "created_at" TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS "users" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "created_at" TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS "pubkeys" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "user_id" INTEGER NOT NULL REFERENCES "users"("id") ON DELETE CASCADE,
    "fingerprint" TEXT NOT NULL UNIQUE,
    "key" TEXT NOT NULL,
    "created_at" TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS "memberships" (
    "user_id" INTEGER NOT NULL REFERENCES "users"("id") ON DELETE CASCADE,
    "tenant_id" TEXT NOT NULL REFERENCES "tenants"("id") ON DELETE CASCADE,
    "role" TEXT NOT NULL CHECK ("role" IN ('owner', 'member')),
    "created_at" TEXT NOT NULL,
    PRIMARY KEY ("user_id", "tenant_id")
);
CREATE INDEX IF NOT EXISTS "memberships_tenant_id_index" ON "memberships" ("tenant_id");
CREATE TABLE IF NOT EXISTS "invites" (
    "code" TEXT PRIMARY KEY,
    "tenant_id" TEXT NOT NULL REFERENCES "tenants"("id") ON DELETE CASCADE,
    "expires_at" TEXT NOT NULL,
    "created_at" TEXT NOT NULL
);
"#;

#[derive(Debug, Clone, PartialEq)]
pub struct Tenant {
    pub id: String,
    pub slug: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Member => "member",
        }
    }

    fn from_db(s: &str) -> Role {
        if s == "owner" {
            Role::Owner
        } else {
            Role::Member
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Invite {
    pub code: String,
    pub tenant_id: String,
    pub expires_at: String,
    pub created_at: String,
}

/// Slugs: 1-64 chars of lowercase ASCII letters, digits, hyphens; must not
/// start with a hyphen (they become file-adjacent identifiers and CLI args).
fn valid_slug(slug: &str) -> bool {
    fn ok(b: u8) -> bool {
        b.is_ascii_lowercase() || b.is_ascii_digit()
    }
    let bytes = slug.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && ok(bytes[0])
        && bytes[1..].iter().all(|&b| ok(b) || b == b'-')
}

fn tenant_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Tenant> {
    Ok(Tenant {
        id: r.get(0)?,
        slug: r.get(1)?,
        created_at: r.get(2)?,
    })
}

impl Registry {
    /// Open (or create) the registry DB at `path` and ensure the schema.
    /// Same connection pragmas as cliban-core's store: WAL, FKs on,
    /// busy_timeout. Pass ":memory:" for tests.
    pub fn open(path: impl AsRef<Path>) -> Result<Registry> {
        let conn = Connection::open(path.as_ref())?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.execute_batch(REGISTRY_DDL)?;
        Ok(Registry {
            conn: Mutex::new(conn),
        })
    }

    /// Run `f` with the (locked) connection. Every public method funnels
    /// through here; the mutex serializes registry access.
    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.conn.lock().expect("registry mutex poisoned");
        f(&conn)
    }

    // -- users + pubkeys ----------------------------------------------------

    pub fn create_user(&self, name: &str) -> Result<User> {
        self.with(|conn| {
            let now = time::format_usec(time::now_usec());
            conn.execute(
                "INSERT INTO users (name, created_at) VALUES (?1, ?2)",
                params![name, now],
            )?;
            Ok(User {
                id: conn.last_insert_rowid(),
                name: name.to_string(),
                created_at: now,
            })
        })
    }

    /// Register an SSH public key for a user. `fingerprint` is the lookup
    /// handle (unique); `key` is the full key blob for reference.
    pub fn add_pubkey(&self, user_id: i64, fingerprint: &str, key: &str) -> Result<()> {
        self.with(|conn| {
            let now = time::format_usec(time::now_usec());
            conn.execute(
                "INSERT INTO pubkeys (user_id, fingerprint, key, created_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![user_id, fingerprint, key, now],
            )?;
            Ok(())
        })
    }

    /// Resolve a key fingerprint to its user (auth entry point).
    pub fn user_for_pubkey(&self, fingerprint: &str) -> Result<Option<User>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    "SELECT u.id, u.name, u.created_at FROM users u \
                     JOIN pubkeys p ON p.user_id = u.id \
                     WHERE p.fingerprint = ?1",
                    params![fingerprint],
                    |r| {
                        Ok(User {
                            id: r.get(0)?,
                            name: r.get(1)?,
                            created_at: r.get(2)?,
                        })
                    },
                )
                .optional()?)
        })
    }

    /// Atomically enroll an SSH key: if the fingerprint is already known,
    /// return its existing user (concurrent enrollments are idempotent —
    /// first one wins); otherwise create the user + pubkey rows in one
    /// transaction, so no partial enrollment can ever be observed.
    pub fn enroll_key(&self, name: &str, fingerprint: &str, key: &str) -> Result<User> {
        self.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            let existing = tx
                .query_row(
                    "SELECT u.id, u.name, u.created_at FROM users u \
                     JOIN pubkeys p ON p.user_id = u.id \
                     WHERE p.fingerprint = ?1",
                    params![fingerprint],
                    |r| {
                        Ok(User {
                            id: r.get(0)?,
                            name: r.get(1)?,
                            created_at: r.get(2)?,
                        })
                    },
                )
                .optional()?;
            if let Some(u) = existing {
                return Ok(u);
            }
            let now = time::format_usec(time::now_usec());
            tx.execute(
                "INSERT INTO users (name, created_at) VALUES (?1, ?2)",
                params![name, now],
            )?;
            let user_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO pubkeys (user_id, fingerprint, key, created_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![user_id, fingerprint, key, now],
            )?;
            tx.commit()?;
            Ok(User {
                id: user_id,
                name: name.to_string(),
                created_at: now,
            })
        })
    }

    // -- tenants ------------------------------------------------------------

    /// Create a tenant owned by `owner_user_id`, enforcing both caps inside
    /// one transaction (checks + insert are atomic under the registry mutex).
    /// The tenant id is 16 random bytes hex-encoded, generated by SQLite's
    /// randomblob so we need no RNG dependency.
    pub fn create_tenant(
        &self,
        slug: &str,
        owner_user_id: i64,
        max_tenants_per_user: i64,
        max_tenants_global: i64,
    ) -> Result<Tenant> {
        if !valid_slug(slug) {
            return Err(Error::InvalidSlug);
        }
        self.with(|conn| {
            let tx = conn.unchecked_transaction()?;

            let global: i64 = tx.query_row("SELECT COUNT(*) FROM tenants", [], |r| r.get(0))?;
            if global >= max_tenants_global {
                return Err(Error::CapExceeded("global tenant count"));
            }
            let owned: i64 = tx.query_row(
                "SELECT COUNT(*) FROM memberships WHERE user_id = ?1 AND role = 'owner'",
                params![owner_user_id],
                |r| r.get(0),
            )?;
            if owned >= max_tenants_per_user {
                return Err(Error::CapExceeded("tenants per user"));
            }
            let taken = tx
                .query_row(
                    "SELECT 1 FROM tenants WHERE slug = ?1",
                    params![slug],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if taken {
                return Err(Error::SlugTaken);
            }

            let id: String = tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
            let now = time::format_usec(time::now_usec());
            tx.execute(
                "INSERT INTO tenants (id, slug, created_at) VALUES (?1, ?2, ?3)",
                params![id, slug, now],
            )?;
            tx.execute(
                "INSERT INTO memberships (user_id, tenant_id, role, created_at) \
                 VALUES (?1, ?2, 'owner', ?3)",
                params![owner_user_id, id, now],
            )?;
            tx.commit()?;
            Ok(Tenant {
                id,
                slug: slug.to_string(),
                created_at: now,
            })
        })
    }

    pub fn tenant(&self, id: &str) -> Result<Option<Tenant>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    "SELECT id, slug, created_at FROM tenants WHERE id = ?1",
                    params![id],
                    tenant_from_row,
                )
                .optional()?)
        })
    }

    pub fn tenant_by_slug(&self, slug: &str) -> Result<Option<Tenant>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    "SELECT id, slug, created_at FROM tenants WHERE slug = ?1",
                    params![slug],
                    tenant_from_row,
                )
                .optional()?)
        })
    }

    pub fn tenant_count(&self) -> Result<i64> {
        self.with(|conn| Ok(conn.query_row("SELECT COUNT(*) FROM tenants", [], |r| r.get(0))?))
    }

    /// Delete the registry rows for a tenant. Memberships and invites go via
    /// ON DELETE CASCADE. File deletion is the manager's job.
    pub fn delete_tenant(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM tenants WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    // -- memberships ----------------------------------------------------------

    pub fn add_membership(&self, user_id: i64, tenant_id: &str, role: Role) -> Result<()> {
        self.with(|conn| {
            let now = time::format_usec(time::now_usec());
            conn.execute(
                "INSERT INTO memberships (user_id, tenant_id, role, created_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![user_id, tenant_id, role.as_str(), now],
            )?;
            Ok(())
        })
    }

    pub fn role(&self, user_id: i64, tenant_id: &str) -> Result<Option<Role>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    "SELECT role FROM memberships WHERE user_id = ?1 AND tenant_id = ?2",
                    params![user_id, tenant_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .map(|s| Role::from_db(&s)))
        })
    }

    pub fn tenants_for_user(&self, user_id: i64) -> Result<Vec<(Tenant, Role)>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.slug, t.created_at, m.role \
                 FROM tenants t JOIN memberships m ON m.tenant_id = t.id \
                 WHERE m.user_id = ?1 ORDER BY t.slug",
            )?;
            let rows = stmt.query_map(params![user_id], |r| {
                Ok((tenant_from_row(r)?, r.get::<_, String>(3)?))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (t, role) = row?;
                out.push((t, Role::from_db(&role)));
            }
            Ok(out)
        })
    }

    /// List a tenant's members with roles, ordered by user name.
    pub fn members(&self, tenant_id: &str) -> Result<Vec<(User, Role)>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT u.id, u.name, u.created_at, m.role \
                 FROM users u JOIN memberships m ON m.user_id = u.id \
                 WHERE m.tenant_id = ?1 ORDER BY u.name",
            )?;
            let rows = stmt.query_map(params![tenant_id], |r| {
                Ok((
                    User {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        created_at: r.get(2)?,
                    },
                    r.get::<_, String>(3)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (u, role) = row?;
                out.push((u, Role::from_db(&role)));
            }
            Ok(out)
        })
    }

    // -- invites --------------------------------------------------------------

    /// Create a one-time invite to a tenant. `expires_at` is a
    /// `cliban_core::time::format_usec` timestamp string; the fixed-width
    /// UTC ISO format compares chronologically as text, so expiry checks are
    /// plain string comparisons. The code is 16 random bytes hex-encoded.
    /// Rejects `expires_at` strings that are not in that exact canonical form
    /// up front (a merely-parseable RFC3339 variant — offset form, missing
    /// fraction — would mis-compare as text), and opportunistically sweeps
    /// already-expired invites (across all tenants) before inserting.
    pub fn create_invite(&self, tenant_id: &str, expires_at: &str) -> Result<Invite> {
        let canonical = time::parse_ts(expires_at).map(time::format_usec);
        if canonical.as_deref() != Some(expires_at) {
            return Err(Error::InvalidExpiry);
        }
        self.with(|conn| {
            let known = conn
                .query_row(
                    "SELECT 1 FROM tenants WHERE id = ?1",
                    params![tenant_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !known {
                return Err(Error::TenantNotFound);
            }
            let now = time::format_usec(time::now_usec());
            // Opportunistic GC: expired invites can never be redeemed, so
            // sweep them whenever a new invite is minted.
            conn.execute("DELETE FROM invites WHERE expires_at <= ?1", params![now])?;
            let code: String =
                conn.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
            conn.execute(
                "INSERT INTO invites (code, tenant_id, expires_at, created_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![code, tenant_id, expires_at, now],
            )?;
            Ok(Invite {
                code,
                tenant_id: tenant_id.to_string(),
                expires_at: expires_at.to_string(),
                created_at: now,
            })
        })
    }

    /// Redeem an invite: consume the code (one-time: the row is deleted) and
    /// add `user_id` as a member. Unknown, already-used, and expired codes
    /// all return [`Error::InviteInvalid`] — indistinguishable on purpose.
    /// Redeeming a tenant you already belong to keeps your existing role.
    pub fn redeem_invite(&self, code: &str, user_id: i64) -> Result<Tenant> {
        self.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            let row: Option<(String, String)> = tx
                .query_row(
                    "SELECT tenant_id, expires_at FROM invites WHERE code = ?1",
                    params![code],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((tenant_id, expires_at)) = row else {
                return Err(Error::InviteInvalid);
            };
            let now = time::format_usec(time::now_usec());
            if expires_at <= now {
                return Err(Error::InviteInvalid);
            }
            tx.execute("DELETE FROM invites WHERE code = ?1", params![code])?;
            tx.execute(
                "INSERT OR IGNORE INTO memberships (user_id, tenant_id, role, created_at) \
                 VALUES (?1, ?2, 'member', ?3)",
                params![user_id, tenant_id, now],
            )?;
            let tenant = tx.query_row(
                "SELECT id, slug, created_at FROM tenants WHERE id = ?1",
                params![tenant_id],
                tenant_from_row,
            )?;
            tx.commit()?;
            Ok(tenant)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn mem() -> Registry {
        Registry::open(":memory:").unwrap()
    }

    #[test]
    fn open_is_idempotent_on_existing_schema() {
        // Two opens against the same file must not fail on existing tables.
        let dir = std::env::temp_dir().join(format!(
            "cliban-tenancy-reopen-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("registry.db");
        let _first = Registry::open(&path).unwrap();
        drop(_first);
        let _second = Registry::open(&path).unwrap();
    }

    #[test]
    fn pubkey_resolves_to_user() {
        let r = mem();
        let u = r.create_user("alice").unwrap();
        r.add_pubkey(u.id, "SHA256:abc", "ssh-ed25519 AAAA... alice@host")
            .unwrap();

        let found = r.user_for_pubkey("SHA256:abc").unwrap();
        assert_eq!(found, Some(u));
        assert_eq!(r.user_for_pubkey("SHA256:nope").unwrap(), None);
    }

    #[test]
    fn duplicate_fingerprint_is_rejected() {
        let r = mem();
        let u = r.create_user("alice").unwrap();
        r.add_pubkey(u.id, "SHA256:abc", "key-a").unwrap();
        assert!(r.add_pubkey(u.id, "SHA256:abc", "key-b").is_err());
    }

    #[test]
    fn enroll_key_is_atomic_and_idempotent() {
        let r = mem();
        let u1 = r.enroll_key("alice", "SHA256:k", "blob").unwrap();
        let u2 = r.enroll_key("alice-again", "SHA256:k", "blob").unwrap();
        assert_eq!(u1, u2); // key already enrolled: existing user wins
        assert_eq!(r.user_for_pubkey("SHA256:k").unwrap(), Some(u1));
    }

    pub(crate) const NO_CAP: i64 = i64::MAX;

    #[test]
    fn create_tenant_round_trips_and_records_owner() {
        let r = mem();
        let u = r.create_user("alice").unwrap();
        let t = r.create_tenant("team-a", u.id, NO_CAP, NO_CAP).unwrap();

        assert_eq!(t.slug, "team-a");
        assert_eq!(t.id.len(), 32); // 16 random bytes, hex-encoded
        assert_eq!(r.tenant(&t.id).unwrap(), Some(t.clone()));
        assert_eq!(r.tenant_by_slug("team-a").unwrap(), Some(t.clone()));
        assert_eq!(r.role(u.id, &t.id).unwrap(), Some(Role::Owner));
        assert_eq!(r.tenant_count().unwrap(), 1);
    }

    #[test]
    fn slugs_are_validated_and_unique() {
        let r = mem();
        let u = r.create_user("alice").unwrap();
        r.create_tenant("team-a", u.id, NO_CAP, NO_CAP).unwrap();

        assert!(matches!(
            r.create_tenant("team-a", u.id, NO_CAP, NO_CAP),
            Err(Error::SlugTaken)
        ));
        let too_long = "x".repeat(65);
        for bad in ["", "Team", "has space", "-lead", too_long.as_str()] {
            assert!(
                matches!(
                    r.create_tenant(bad, u.id, NO_CAP, NO_CAP),
                    Err(Error::InvalidSlug)
                ),
                "slug {bad:?} should be invalid"
            );
        }
    }

    #[test]
    fn caps_reject_creation_atomically() {
        let r = mem();
        let alice = r.create_user("alice").unwrap();
        let bob = r.create_user("bob").unwrap();

        // Per-user cap of 1.
        r.create_tenant("a1", alice.id, 1, NO_CAP).unwrap();
        assert!(matches!(
            r.create_tenant("a2", alice.id, 1, NO_CAP),
            Err(Error::CapExceeded(_))
        ));
        // Rejected create leaves no rows behind.
        assert_eq!(r.tenant_by_slug("a2").unwrap(), None);
        assert_eq!(r.tenant_count().unwrap(), 1);

        // Global cap of 2: bob's first is fine, second hits the global cap
        // even though his per-user count allows it.
        r.create_tenant("b1", bob.id, NO_CAP, 2).unwrap();
        assert!(matches!(
            r.create_tenant("b2", bob.id, NO_CAP, 2),
            Err(Error::CapExceeded(_))
        ));
    }

    #[test]
    fn memberships_list_and_role() {
        let r = mem();
        let alice = r.create_user("alice").unwrap();
        let bob = r.create_user("bob").unwrap();
        let t = r.create_tenant("team-a", alice.id, NO_CAP, NO_CAP).unwrap();

        assert_eq!(r.role(bob.id, &t.id).unwrap(), None);
        r.add_membership(bob.id, &t.id, Role::Member).unwrap();
        assert_eq!(r.role(bob.id, &t.id).unwrap(), Some(Role::Member));

        let bobs = r.tenants_for_user(bob.id).unwrap();
        assert_eq!(bobs, vec![(t.clone(), Role::Member)]);
    }

    #[test]
    fn delete_tenant_cascades_memberships() {
        let r = mem();
        let alice = r.create_user("alice").unwrap();
        let t = r.create_tenant("team-a", alice.id, NO_CAP, NO_CAP).unwrap();

        r.delete_tenant(&t.id).unwrap();
        assert_eq!(r.tenant(&t.id).unwrap(), None);
        assert_eq!(r.role(alice.id, &t.id).unwrap(), None);
        assert_eq!(r.tenant_count().unwrap(), 0);
    }

    /// A timestamp far in the future / past, in the registry's own format.
    fn ts_in(days: i64) -> String {
        cliban_core::time::format_usec(cliban_core::time::now_usec() + chrono::Duration::days(days))
    }

    #[test]
    fn invite_redeems_once_and_adds_member() {
        let r = mem();
        let alice = r.create_user("alice").unwrap();
        let bob = r.create_user("bob").unwrap();
        let t = r.create_tenant("team-a", alice.id, NO_CAP, NO_CAP).unwrap();

        let inv = r.create_invite(&t.id, &ts_in(1)).unwrap();
        assert_eq!(inv.tenant_id, t.id);
        assert_eq!(inv.code.len(), 32);

        let joined = r.redeem_invite(&inv.code, bob.id).unwrap();
        assert_eq!(joined, t);
        assert_eq!(r.role(bob.id, &t.id).unwrap(), Some(Role::Member));

        // One-time: the second redemption fails, even for another user.
        let carol = r.create_user("carol").unwrap();
        assert!(matches!(
            r.redeem_invite(&inv.code, carol.id),
            Err(Error::InviteInvalid)
        ));
    }

    #[test]
    fn expired_and_unknown_invites_are_rejected() {
        let r = mem();
        let alice = r.create_user("alice").unwrap();
        let bob = r.create_user("bob").unwrap();
        let t = r.create_tenant("team-a", alice.id, NO_CAP, NO_CAP).unwrap();

        let expired = r.create_invite(&t.id, &ts_in(-1)).unwrap();
        assert!(matches!(
            r.redeem_invite(&expired.code, bob.id),
            Err(Error::InviteInvalid)
        ));
        assert!(matches!(
            r.redeem_invite("deadbeef", bob.id),
            Err(Error::InviteInvalid)
        ));
        assert_eq!(r.role(bob.id, &t.id).unwrap(), None);
    }

    #[test]
    fn invite_for_unknown_tenant_is_rejected() {
        let r = mem();
        assert!(matches!(
            r.create_invite("nope", &ts_in(1)),
            Err(Error::TenantNotFound)
        ));
    }

    #[test]
    fn redeem_by_existing_member_is_idempotent() {
        let r = mem();
        let alice = r.create_user("alice").unwrap();
        let t = r.create_tenant("team-a", alice.id, NO_CAP, NO_CAP).unwrap();

        // Owner redeems an invite to their own tenant: consumed, role kept.
        let inv = r.create_invite(&t.id, &ts_in(1)).unwrap();
        r.redeem_invite(&inv.code, alice.id).unwrap();
        assert_eq!(r.role(alice.id, &t.id).unwrap(), Some(Role::Owner));
    }

    #[test]
    fn members_lists_users_with_roles() {
        let r = mem();
        let alice = r.create_user("alice").unwrap();
        let bob = r.create_user("bob").unwrap();
        let t = r.create_tenant("team-a", alice.id, NO_CAP, NO_CAP).unwrap();
        r.add_membership(bob.id, &t.id, Role::Member).unwrap();

        let m = r.members(&t.id).unwrap();
        assert_eq!(m, vec![(alice, Role::Owner), (bob, Role::Member)]);
        assert_eq!(r.members("nope").unwrap(), vec![]);
    }

    #[test]
    fn create_invite_rejects_malformed_expiry() {
        let r = mem();
        let alice = r.create_user("alice").unwrap();
        let t = r.create_tenant("team-a", alice.id, NO_CAP, NO_CAP).unwrap();
        // Not a timestamp at all, plus parseable-but-non-canonical RFC3339
        // forms (offset, no fraction) that would break lexicographic expiry
        // comparison. Only the exact format_usec form is accepted.
        for bad in [
            "next tuesday",
            "2027-01-01T00:00:00+05:00",
            "2027-01-01T00:00:00Z",
        ] {
            assert!(
                matches!(r.create_invite(&t.id, bad), Err(Error::InvalidExpiry)),
                "expiry {bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn creating_an_invite_gcs_expired_rows() {
        let r = mem();
        let alice = r.create_user("alice").unwrap();
        let t = r.create_tenant("team-a", alice.id, NO_CAP, NO_CAP).unwrap();

        let expired = r.create_invite(&t.id, &ts_in(-1)).unwrap();
        let live = r.create_invite(&t.id, &ts_in(1)).unwrap();

        // The second create_invite sweeps the expired row; only `live` remains.
        let codes: Vec<String> = r
            .with(|c| {
                let mut stmt = c.prepare("SELECT code FROM invites")?;
                let rows = stmt.query_map([], |row| row.get(0))?;
                Ok(rows.collect::<rusqlite::Result<Vec<String>>>()?)
            })
            .unwrap();
        assert_eq!(codes, vec![live.code.clone()]);
        assert_ne!(codes[0], expired.code);
    }
}
