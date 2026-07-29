//! `~/.config/cliban/linear.toml` — the optional bits of Linear setup.
//!
//! Optional is the point: with no config file at all, `import` works and
//! `push` works on any already-linked issue, because the status map falls back
//! to Linear's own workflow-state *types* (see [`crate::linear::states`]). The
//! file exists for the two things that cannot be inferred — which team new
//! issues go to, and a state name that does not match cliban's vocabulary.
//!
//! The API token is deliberately **not** a config field. It lives in
//! `$LINEAR_API_KEY` and nowhere else, so there is no cliban-owned file on disk
//! that is worth stealing and no path by which a token reaches a log, a
//! `--json` payload, or a git repo full of dotfiles.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

/// Environment variable holding the Linear API key.
pub const TOKEN_ENV: &str = "LINEAR_API_KEY";

/// Config file name inside [`cliban_core::paths::config_dir`].
pub const FILE_NAME: &str = "linear.toml";

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub linear: LinearConfig,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct LinearConfig {
    /// Team key (e.g. `ENG`) new issues are created in when `push --create` is
    /// used without `--team`.
    pub team: Option<String>,
    /// cliban status → exact Linear workflow-state name. Overrides the
    /// name-then-type inference for the statuses listed; anything absent still
    /// infers.
    pub states: BTreeMap<String, String>,
}

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        let cfg: Config = toml::from_str(text).map_err(|e| Error::Config(e.to_string()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Load from `path`, or return defaults when the file is absent. A file
    /// that exists but cannot be read or parsed is an error — silently falling
    /// back to defaults there would apply the wrong state map without saying so.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::Config(format!("{}: {e}", path.display()))),
        }
    }

    /// Load from the default location.
    pub fn load_default() -> Result<Self> {
        Self::load(&default_path())
    }

    /// Reject state overrides keyed on something that is not a cliban status —
    /// a typo like `in_review` would otherwise sit there doing nothing.
    fn validate(&self) -> Result<()> {
        for key in self.linear.states.keys() {
            if !cliban_core::schema::ISSUE_STATUSES.contains(&key.as_str()) {
                return Err(Error::Config(format!(
                    "[linear.states] has key {key:?}, which is not a cliban status \
                     (expected one of: {})",
                    cliban_core::schema::ISSUE_STATUSES.join(", ")
                )));
            }
        }
        Ok(())
    }
}

/// `$XDG_CONFIG_HOME/cliban/linear.toml`, falling back to `~/.config/...`.
pub fn default_path() -> PathBuf {
    cliban_core::paths::config_dir().join(FILE_NAME)
}

/// The API token from the environment. Blank counts as unset — an exported but
/// empty variable is a mistake, not a credential.
pub fn token() -> Result<String> {
    token_from(std::env::var(TOKEN_ENV).ok())
}

/// The pure half of [`token`]. Split out so the blank/absent rules can be
/// tested without mutating process-wide environment, which races with every
/// other test in the binary.
pub fn token_from(raw: Option<String>) -> Result<String> {
    raw.map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or(Error::MissingToken(TOKEN_ENV))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_valid_and_all_defaults() {
        let cfg = Config::parse("").unwrap();
        assert_eq!(cfg, Config::default());
        assert!(cfg.linear.team.is_none());
        assert!(cfg.linear.states.is_empty());
    }

    #[test]
    fn parses_team_and_state_overrides() {
        let cfg = Config::parse(
            r#"
            [linear]
            team = "ENG"
            [linear.states]
            in-review = "Code Review"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.linear.team.as_deref(), Some("ENG"));
        assert_eq!(
            cfg.linear.states.get("in-review").map(String::as_str),
            Some("Code Review")
        );
    }

    #[test]
    fn rejects_a_state_key_that_is_not_a_cliban_status() {
        let err = Config::parse(
            r#"
            [linear.states]
            in_review = "Code Review"
            "#,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("in_review"), "{msg}");
        assert!(
            msg.contains("in-review"),
            "should list the valid set: {msg}"
        );
    }

    #[test]
    fn rejects_unknown_keys_rather_than_ignoring_them() {
        // deny_unknown_fields: a misspelled key that silently did nothing
        // would be worse than a loud parse failure.
        assert!(Config::parse("[linear]\nteem = \"ENG\"\n").is_err());
    }

    #[test]
    fn a_missing_file_is_defaults_not_an_error() {
        let cfg = Config::load(Path::new("/nonexistent/cliban/linear.toml")).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn token_treats_blank_and_absent_alike() {
        assert!(token_from(None).is_err());
        assert!(token_from(Some(String::new())).is_err());
        assert!(token_from(Some("   ".into())).is_err());
        assert_eq!(
            token_from(Some("  lin_api_xyz  ".into())).unwrap(),
            "lin_api_xyz"
        );
    }

    #[test]
    fn missing_token_error_names_the_variable_and_where_to_get_one() {
        let msg = token_from(None).unwrap_err().to_string();
        assert!(msg.contains(TOKEN_ENV), "{msg}");
        assert!(msg.contains("linear.app/settings/api"), "{msg}");
    }
}
