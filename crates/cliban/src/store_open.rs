use cliban_core::{Store, paths};
use std::path::PathBuf;

/// Resolve the DB path: --db flag, else $CLIBAN_DB, else XDG default.
#[allow(dead_code)]
pub fn db_path(flag: &Option<String>) -> PathBuf {
    if let Some(p) = flag { if !p.is_empty() { return PathBuf::from(p); } }
    paths::db_path()
}

/// Open the store at the resolved path (creates dir + migrates).
///
/// `Store::open` is synchronous (it blocks the calling thread until the writer
/// thread reports ready), so there is nothing to `.await` here. `open` stays
/// `async` for call-site uniformity and returns the value directly.
#[allow(dead_code)]
pub async fn open(flag: &Option<String>) -> cliban_core::Result<Store> {
    Store::open(db_path(flag))
}
