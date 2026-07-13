//! `cliband` daemon configuration, loaded from a TOML file.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ServerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignupPolicy {
    Open,
    Token,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Address the SSH listener binds, e.g. "0.0.0.0:2222".
    pub listen_addr: String,
    /// Directory for the host key (and, in later tickets, tenant data).
    pub data_dir: PathBuf,
    /// Who may sign up; enforced by the signup control command.
    pub signup_policy: SignupPolicy,
    /// Shared token required when `signup_policy = "token"`. When the policy
    /// is "token" and this is unset (the default), signup is denied entirely.
    pub signup_token: Option<String>,
    /// Max tenants a single public key may create. 0 = unlimited.
    pub max_tenants_per_key: u32,
    /// Global tenant cap. 0 = unlimited.
    pub max_tenants: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:2222".into(),
            data_dir: default_data_dir(),
            signup_policy: SignupPolicy::Token,
            signup_token: None,
            max_tenants_per_key: 5,
            max_tenants: 0,
        }
    }
}

impl ServerConfig {
    pub fn parse(text: &str) -> Result<Self, ServerError> {
        Ok(toml::from_str(text)?)
    }

    pub fn load(path: &Path) -> Result<Self, ServerError> {
        Self::parse(&std::fs::read_to_string(path)?)
    }
}

/// `$XDG_DATA_HOME/cliband`, falling back to `~/.local/share/cliband`.
/// (Same shape as `cliban_core::paths::data_dir`, but the daemon keeps its
/// own directory name.)
fn default_data_dir() -> PathBuf {
    match std::env::var("XDG_DATA_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v).join("cliband"),
        _ => {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("cliband")
        }
    }
}
