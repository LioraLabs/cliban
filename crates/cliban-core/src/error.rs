//! Error type for the store + contexts.
//!
//! Mirrors the Elixir contexts' error vocabulary: changeset-style validation
//! failures become [`Error::Validation`] (a list of `(field, message)`
//! pairs, matching `Ecto.Changeset.errors`), domain rejections become typed
//! variants (e.g. [`Error::ProjectNotFound`], [`Error::NotFound`]), and
//! anything from rusqlite/serde bubbles up untyped.

use thiserror::Error;

/// A single changeset-style validation error: the field name and a message.
/// Mirrors one entry of `Ecto.Changeset.errors`.
pub type FieldError = (String, String);

#[derive(Debug, Error)]
pub enum Error {
    /// One or more changeset-style validation failures. The Elixir side
    /// returns `{:error, %Ecto.Changeset{errors: [...]}}`; we carry the
    /// equivalent `(field, message)` pairs so callers (and tests) can assert
    /// on the same field/message contract.
    #[error("validation failed: {0:?}")]
    Validation(Vec<FieldError>),

    /// `{:error, :project_not_found}` / `Repo.rollback(:project_not_found)`.
    #[error("not found: {0}")]
    ProjectNotFound(String),

    /// A lookup that can name the missing identity.
    #[error("not found: {0}")]
    NamedNotFound(String),

    /// `{:error, :not_found}`.
    #[error("not found")]
    NotFound,

    /// The writer task is gone (channel closed). Should not happen in normal
    /// operation; surfaced so callers can fail loudly rather than hang.
    #[error("store writer task unavailable")]
    WriterGone,

    /// The `schema_migrations` ledger holds a version this build has never
    /// heard of and that is *older* than its own — the database was not
    /// written by any cliban we recognize. (A *newer* version is fine; see
    /// [`crate::migrations::run`].)
    #[error(
        "unrecognized database schema version(s) {found:?} (this build expects {expected}); \
         the database at this path was not written by a known cliban"
    )]
    SchemaUnknown { found: Vec<i64>, expected: i64 },

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Construct a single-field validation error, mirroring
    /// `Ecto.Changeset.add_error(cs, field, msg)`.
    pub fn validation(field: &str, message: &str) -> Self {
        Error::Validation(vec![(field.to_string(), message.to_string())])
    }

    /// True if this is a validation error mentioning `field` with a message
    /// that contains `needle`. Test convenience that mirrors the Elixir tests'
    /// `cs.errors[:field] =~ needle` assertions.
    pub fn has_validation(&self, field: &str, needle: &str) -> bool {
        match self {
            Error::Validation(errs) => errs.iter().any(|(f, m)| f == field && m.contains(needle)),
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
