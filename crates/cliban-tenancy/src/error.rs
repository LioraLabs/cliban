//! Error type for the tenancy layer.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// The tenant id is not present in the registry.
    #[error("tenant not found")]
    TenantNotFound,

    /// A tenant with this slug already exists.
    #[error("slug has already been taken")]
    SlugTaken,

    /// Slug failed validation (1-64 chars, lowercase letters/digits/hyphens,
    /// must start with a letter or digit).
    #[error("invalid slug")]
    InvalidSlug,

    /// Invite code unknown, already redeemed, or expired. One variant on
    /// purpose: callers must not be able to distinguish (no code probing).
    #[error("invite invalid or expired")]
    InviteInvalid,

    /// Invite expiry string is not a registry-format timestamp.
    #[error("invalid invite expiry timestamp")]
    InvalidExpiry,

    /// A tenant cap was hit; the payload names which one.
    #[error("tenant cap exceeded: {0}")]
    CapExceeded(&'static str),

    #[error(transparent)]
    Core(#[from] cliban_core::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
