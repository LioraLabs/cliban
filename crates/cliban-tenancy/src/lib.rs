//! `cliban-tenancy` — multi-tenant storage for a hosted cliban server.
//!
//! Physical multi-tenancy: one standard cliban-core SQLite file per tenant
//! (`<data>/tenants/<tenant-id>.db`, created via the existing core migrations
//! on first open) plus a small cross-tenant registry (`<data>/registry.db`)
//! holding tenants, users, pubkeys, memberships, and invites.
//!
//! No UI and no server in this crate; cliban-server wires it up in a later
//! ticket.

pub mod error;
pub mod manager;
pub mod registry;

pub use error::{Error, Result};
pub use manager::{Caps, TenantHandle, TenantManager};
pub use registry::{Invite, Registry, Role, Tenant, User};
