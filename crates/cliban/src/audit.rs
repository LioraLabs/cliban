//! Re-export of `cliban_core::audit`. The recording helpers moved into core
//! so the TUI's board mutations write the same audit trail the CLI does —
//! the activity page would otherwise miss everything done from the board.

pub use cliban_core::audit::*;
