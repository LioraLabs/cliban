//! The store: a [`Store`] handle (Clone + Send + Sync) over a single writer
//! thread that owns the one rusqlite [`Connection`].
//!
//! The handle holds an mpsc sender; a background worker owns the resource and
//! serves jobs one at a time. SQLite is a single writer regardless, so
//! funnelling every read and write through one connection is both correct and
//! the simplest thing that preserves the Elixir `Repo.transaction` semantics —
//! each job runs to completion before the next starts, so a context function
//! that opens a transaction has the connection entirely to itself.
//!
//! rusqlite is blocking and `Connection` is `!Sync`, so the worker is a
//! dedicated OS thread (not a tokio task). Async callers submit a closure +
//! await a `oneshot`; the closure runs on the worker thread with `&Connection`
//! and returns a value back through the channel. WAL is enabled on open.

use std::path::Path;
use std::sync::mpsc as std_mpsc;
use std::thread;

use rusqlite::Connection;
use tokio::sync::oneshot;

use crate::error::{Error, Result};

/// A unit of work for the writer thread: a boxed closure given the live
/// connection. We erase the return type into the closure itself (it owns its
/// own oneshot sender), so the worker loop stays monomorphic.
type Job = Box<dyn FnOnce(&Connection) + Send + 'static>;

/// Clone-able handle to the store. Cheap to clone (just an `mpsc::Sender`);
/// safe to share across tasks and threads. Dropping all clones shuts the
/// worker down.
#[derive(Clone)]
pub struct Store {
    tx: std_mpsc::Sender<Job>,
}

impl Store {
    /// Open (or create) the DB at `path`, run migrations, enable WAL, and spawn
    /// the writer thread. Returns once the worker is ready (the open +
    /// migration happen synchronously on the worker and the result is awaited).
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            // Make the data dir if the DB lives somewhere that doesn't exist
            // yet. Ignore failures here; the open below will surface a real
            // error.
            let _ = std::fs::create_dir_all(parent);
        }
        Self::spawn(move || Connection::open(&path))
    }

    /// Open the store at the default [`crate::paths::db_path`] location. The CLI
    /// and TUI entry points will call this; tests use
    /// [`Store::open_in_memory`] instead.
    pub fn open_default() -> Result<Store> {
        Self::open(crate::paths::db_path())
    }

    /// Open an in-memory store. Test convenience; the contents vanish on drop.
    pub fn open_in_memory() -> Result<Store> {
        Self::spawn(Connection::open_in_memory)
    }

    fn spawn<F>(open: F) -> Result<Store>
    where
        F: FnOnce() -> rusqlite::Result<Connection> + Send + 'static,
    {
        let (tx, rx) = std_mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = std_mpsc::channel::<Result<()>>();

        thread::Builder::new()
            .name("cliban-store".into())
            .spawn(move || {
                let conn = match open().map_err(Error::from).and_then(init_connection) {
                    Ok(c) => {
                        let _ = ready_tx.send(Ok(()));
                        c
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };

                // Serve jobs until every handle is dropped.
                while let Ok(job) = rx.recv() {
                    job(&conn);
                }
            })
            .expect("spawn cliban-store thread");

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Store { tx }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::WriterGone),
        }
    }

    /// Submit `f` to run on the writer thread with exclusive access to the
    /// connection, and await its result. This is the single primitive every
    /// context method is built on; a context function that needs a transaction
    /// simply opens one inside `f` (it has the connection to itself for the
    /// duration of the call).
    pub async fn call<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel::<Result<T>>();
        let job: Job = Box::new(move |conn| {
            let out = f(conn);
            let _ = reply_tx.send(out);
        });
        self.tx.send(job).map_err(|_| Error::WriterGone)?;
        reply_rx.await.map_err(|_| Error::WriterGone)?
    }
}

/// Per-connection setup, run once on open. WAL for concurrent readers, foreign
/// keys ON (Ecto runs with `PRAGMA foreign_key = ON` per-connection;
/// ecto_sqlite3 enables it by default), busy_timeout so a momentarily-locked
/// DB retries instead of erroring, and the migration baseline.
fn init_connection(conn: Connection) -> Result<Connection> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    crate::migrations::run(&conn)?;
    Ok(conn)
}
