//! The Linear bridge.
//!
//! Layered so the parts that can be tested without a network are the parts
//! that can do damage:
//!
//! - [`client`] — the only code that talks HTTP, behind a one-method trait.
//! - [`model`] — serde shapes for the fields cliban mirrors.
//! - [`ops`] — typed GraphQL operations, generic over the trait.
//! - [`states`] — cliban's five statuses ↔ a team's workflow states.
//! - [`render`] — the description and comment text, pure string in/out.

pub mod client;
pub mod model;
pub mod ops;
pub mod render;
pub mod states;

pub use client::{Client, GraphQl};
