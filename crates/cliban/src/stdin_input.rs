//! Where a command's text payload comes from.
//!
//! Two ways in, and both live here so every command answers them the same
//! way. [`resolve`] handles the explicit pair — a value flag and its
//! `--*-file` sibling, with a bare `-` meaning stdin on either.
//! [`fallback`] handles the implicit one: when the primary text argument is
//! absent AND stdin is piped/redirected, the text comes from stdin — `echo
//! note | cliban issue log KEY` just works, no `--message-file -`
//! incantation.
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
    if std::io::stdin().is_terminal() {
        return Ok(None);
    }
    read_stdin().map(Some)
}

/// Resolves a text payload from a value flag and its `--*-file` sibling.
///
/// The two are mutually exclusive, and a bare `-` on *either* is the stdin
/// sentinel — `--body -` and `--body-file -` both read the pipe. Only an
/// exact `-` counts, so a markdown bullet (`- a lesson`) stays literal.
///
/// `Ok(None)` means the caller passed neither. What that means is the
/// caller's contract, not this function's: leave-unchanged for `edit`, empty
/// for `add`, or defer to [`fallback`] for `project note add`.
///
/// `value_flag` / `file_flag` name the flags in the mutual-exclusion error,
/// so `note add` says `--body` rather than some other command's spelling.
///
/// The sentinel belongs to *flags*, not positionals: `issue log KEY -` logs a
/// dash, because a positional payload has no second spelling to disambiguate
/// it from. Those commands read their file arm through [`read_stdin`] and
/// their bare pipe through [`fallback`].
pub fn resolve(
    value: Option<String>,
    file: Option<String>,
    value_flag: &str,
    file_flag: &str,
) -> CliResult<Option<String>> {
    match (value, file) {
        (Some(_), Some(_)) => Err(CliError::validation(format!(
            "{value_flag} and {file_flag} are mutually exclusive"
        ))),
        (Some(v), None) if v == "-" => Ok(Some(read_stdin()?)),
        (Some(v), None) => Ok(Some(v)),
        (None, Some(p)) if p == "-" => Ok(Some(read_stdin()?)),
        (None, Some(path)) => std::fs::read_to_string(path)
            .map(Some)
            .map_err(|e| CliError::validation(e.to_string())),
        (None, None) => Ok(None),
    }
}

/// Unconditional read of stdin — the `-` sentinel says the user meant it, so
/// there is no `is_terminal` check here and a bare `-` at a terminal waits
/// for input, as `cat -` does.
pub fn read_stdin() -> CliResult<String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::other(format!("read stdin: {e}")))?;
    Ok(buf)
}
