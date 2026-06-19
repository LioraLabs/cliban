//! XDG-aware paths for the cliban SQLite DB.

use std::path::PathBuf;

/// `$XDG_DATA_HOME/cliban`, falling back to `~/.local/share/cliban`.
pub fn data_dir() -> PathBuf {
    match std::env::var("XDG_DATA_HOME") {
        Ok(val) if !val.is_empty() => PathBuf::from(val).join("cliban"),
        _ => home_dir().join(".local").join("share").join("cliban"),
    }
}

/// `$CLIBAN_DB` if set, otherwise `data_dir()/cliban.db`.
pub fn db_path() -> PathBuf {
    match std::env::var("CLIBAN_DB") {
        Ok(val) if !val.is_empty() => PathBuf::from(val),
        _ => data_dir().join("cliban.db"),
    }
}

/// Reads `$HOME`.
pub fn home_dir() -> PathBuf {
    match std::env::var("HOME") {
        Ok(val) if !val.is_empty() => PathBuf::from(val),
        _ => PathBuf::from("/"),
    }
}
