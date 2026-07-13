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
    #[error("project not found")]
    ProjectNotFound,

    /// `{:error, :not_found}`.
    #[error("not found")]
    NotFound,

    /// The writer task is gone (channel closed). Should not happen in normal
    /// operation; surfaced so callers can fail loudly rather than hang.
    #[error("store writer task unavailable")]
    WriterGone,

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
