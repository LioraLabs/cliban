//! Stdin fallback for primary text inputs.
//!
//! When a command's primary text argument is absent AND stdin is
//! piped/redirected, the text comes from stdin — `echo note | cliban issue
//! log KEY` just works, no `--message-file -` incantation.
//!
//! This is the binary's one sanctioned `is_terminal()` check on STDIN. It is
//! deliberately separate from the output contract in `crate::output`: that
//! resolver decides how results are *printed* (and owns the stdout check);
//! this one only decides whether piped *input* exists to read. Callers must
//! consult it ONLY when every explicit source (positional, `--*-file`) is
//! absent — explicit input always wins — and a TTY returns `None` so the
//! caller keeps its fast "required" validation error instead of blocking on
//! input the user never intended to type.

use crate::errors::{CliError, CliResult};
use std::io::{IsTerminal, Read};

/// `Ok(None)` when stdin is a terminal; otherwise the full piped/redirected
/// contents. An empty pipe yields `Some("")` — whether that is a validation
/// error (log, append-section) or simply "no body" (project note add) is the
/// caller's contract, not ours.
pub fn fallback() -> CliResult<Option<String>> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }
    let mut buf = String::new();
    stdin
        .lock()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::other(format!("read stdin: {e}")))?;
    Ok(Some(buf))
}
