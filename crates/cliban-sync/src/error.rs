//! Error vocabulary for the sync bridges.
//!
//! Kept separate from [`cliban_core::Error`] because the failure modes are
//! different in kind: the core fails on validation and storage, a bridge fails
//! on credentials, network, and a remote API disagreeing with us. The CLI maps
//! these onto cliban's exit codes (1 not-found, 2 validation, 3 other).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// No API token in the environment. Named explicitly because it is by far
    /// the most common first-run failure and the fix is one line.
    #[error("{0} is not set; export a Linear API key (https://linear.app/settings/api)")]
    MissingToken(&'static str),

    /// The remote rejected our credentials.
    #[error("Linear rejected the API key (HTTP {0})")]
    Unauthorized(u16),

    /// The remote returned GraphQL-level errors. Carries their messages
    /// verbatim — they are usually actionable and we have nothing to add.
    #[error("Linear API error: {}", .0.join("; "))]
    Api(Vec<String>),

    /// A lookup found nothing. `what` reads like `issue ENG-412` or `team ENG`.
    #[error("not found: {0}")]
    NotFound(String),

    /// The remote's shape did not match what we asked for. Distinct from
    /// [`Error::Api`]: this one means our query and our structs disagree,
    /// which is a cliban bug rather than a user error.
    #[error("unexpected response from Linear: {0}")]
    Unexpected(String),

    /// A local edit would clobber a newer remote change.
    #[error(
        "{remote_key} changed in Linear since the last sync \
         (remote {remote}, ours {ours}); re-import first, or pass --force"
    )]
    StaleWrite {
        remote_key: String,
        remote: String,
        ours: String,
    },

    /// The config file exists but does not parse or does not make sense.
    #[error("config: {0}")]
    Config(String),

    #[error("HTTP transport: {0}")]
    Http(String),

    #[error(transparent)]
    Core(#[from] cliban_core::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
