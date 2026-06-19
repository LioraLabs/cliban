//! `cliban-core` — cliban's storage + domain layer in Rust.
//!
//! Lifted and trimmed from loom-core: the [`Store`] writer-thread actor over a
//! single rusqlite connection, plus domain contexts for projects, milestones,
//! issues, labels, relations, and the per-issue activity log. No daemon, no
//! sockets — the CLI and TUI link this crate and open the SQLite file in
//! process.

pub mod contexts;
pub mod error;
pub mod migrations;
pub mod paths;
pub mod projection;
pub mod rows;
pub mod schema;
pub mod store;
pub mod time;

pub use error::{Error, Result};
pub use store::Store;
