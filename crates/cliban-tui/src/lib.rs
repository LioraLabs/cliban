//! cliban TUI — loom's ratatui frontend, rewired to in-process `cliban-core`.

pub mod actions;
pub mod app;
pub mod buffers;
pub mod data;
pub mod keybinds;
pub mod runtime;
pub mod ui;

use std::path::Path;

/// Open the DB at `path` and launch the TUI against it. Blocks until the user
/// quits. This is the entry the `cliban` binary will call for `cliban board`.
pub fn run(db_path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
    runtime::run(db_path.as_ref())
}
