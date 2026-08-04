//! `$CLIBAN_PROJECT` — the ambient project scope.
//!
//! A repo bound to one board exports its project key once (direnv, a shell
//! profile, a harness env) and every command that takes `-p` inherits it:
//! `cliban issue add "Fix ordering"` lands in the right project without the
//! flag. Precedence is strict: an explicit `-p` always wins, and `-p '*'`
//! deliberately widens back out to every project — needed because once an
//! env var scopes reads by default, "all projects" requires a spelling of
//! its own. `*` can never collide with a project key (keys are uppercase
//! alphanumerics).

use crate::errors::{CliError, CliResult};

/// Resolve an optional `-p/--project` against `$CLIBAN_PROJECT`.
///
/// Returns the upper-cased key, or `None` for genuinely unscoped (no flag and
/// no env, or an explicit `-p '*'`).
pub fn project(flag: Option<String>) -> Option<String> {
    match flag {
        Some(s) if s.trim() == "*" => None,
        Some(s) if !s.trim().is_empty() => Some(s.trim().to_uppercase()),
        _ => std::env::var("CLIBAN_PROJECT")
            .ok()
            .map(|s| s.trim().to_uppercase())
            .filter(|s| !s.is_empty() && s != "*"),
    }
}

/// Same, for commands that cannot run unscoped (`issue add`, `milestone
/// add`, …). The error names both spellings so a bare invocation teaches the
/// contract.
pub fn required_project(flag: Option<String>) -> CliResult<String> {
    project(flag)
        .ok_or_else(|| CliError::validation("no project scope: pass -p KEY or set $CLIBAN_PROJECT"))
}

/// A project addressed by positional KEY where the ambient scope may stand
/// in: reads (`show`, `cat`, `search`) and memory appends (`note add`).
/// Structural writes (`edit`, `archive`, `rm`) never come here — they name
/// their target explicitly.
pub fn project_identity(pos: Option<String>) -> CliResult<String> {
    match pos {
        Some(k) if !k.trim().is_empty() && k.trim() != "*" => Ok(k.trim().to_uppercase()),
        _ => project(None)
            .ok_or_else(|| CliError::validation("no project: pass a KEY or set $CLIBAN_PROJECT")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Env-dependent paths are pinned end-to-end in tests/scope.rs; these
    // cover the pure flag layer.
    #[test]
    fn explicit_flag_wins_and_is_upcased() {
        assert_eq!(project(Some("cli".into())), Some("CLI".into()));
    }

    #[test]
    fn star_means_unscoped() {
        assert_eq!(project(Some("*".into())), None);
    }
}
