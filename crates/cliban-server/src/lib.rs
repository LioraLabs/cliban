//! cliban-server: russh-based SSH daemon (`cliband`) for hosted shared boards.

pub mod commands;
pub mod config;
pub mod hostkey;
pub mod server;
pub mod shell;

/// One error enum for the whole crate — config load, key handling, io.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ssh key error: {0}")]
    Key(#[from] russh::keys::ssh_key::Error),
    #[error("config parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("ssh error: {0}")]
    Russh(#[from] russh::Error),
    #[error(transparent)]
    Tenancy(#[from] cliban_tenancy::Error),
}
