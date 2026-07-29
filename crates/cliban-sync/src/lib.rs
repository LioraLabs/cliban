//! `cliban-sync` — the bridge between a cliban board and an external issue
//! tracker.
//!
//! Deliberately *not* a sync engine. There is no daemon, no polling, and no
//! merge algorithm; there are two explicit verbs a human or agent invokes on
//! one issue at a time — import (remote → cliban) and push (cliban → remote) —
//! and a [`links`] table recording which local issue corresponds to which
//! remote one.
//!
//! What makes that tractable is declared field ownership rather than
//! reconciliation. The remote owns title, priority, labels, due date, workflow
//! state, and the `## Spec` prose. cliban owns `## Plan`, `## Activity Log`,
//! and `## Notes` — the parts that have no counterpart upstream and are the
//! whole reason the board exists. Neither side ever merges the other's fields,
//! so the only conflict left is "the remote moved since we last looked", which
//! is a timestamp comparison (see `linear::push`).

pub mod config;
pub mod error;
pub mod linear;
pub mod links;

pub use error::{Error, Result};

/// Provider key stored in `remote_links.provider`.
pub const PROVIDER_LINEAR: &str = "linear";

/// Entity key stored in `remote_links.entity`. Only issues are linked today;
/// the column exists so milestones or projects can be added without a
/// migration.
pub const ENTITY_ISSUE: &str = "issue";

/// Stable hash of the remote-owned field values, stored as
/// `remote_links.base_hash`.
///
/// What it is for: a refresh overwrites the fields the remote owns, and we would
/// like to tell the user when that overwrite is about to discard something they
/// typed locally. Storing the whole prior state would do it; a hash is enough,
/// because the only question asked is "are these bytes the ones we wrote last
/// time?".
///
/// Deliberately excludes status. Status is *expected* to diverge — an agent
/// moving an issue through the board is the entire point — so including it would
/// make the warning fire on every refresh and teach people to ignore it.
pub fn fingerprint(parts: &[&str]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for part in parts {
        // Length-prefixed so ("ab", "c") and ("a", "bc") do not collide.
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::fingerprint;

    #[test]
    fn fingerprint_is_stable_and_order_sensitive() {
        assert_eq!(fingerprint(&["a", "b"]), fingerprint(&["a", "b"]));
        assert_ne!(fingerprint(&["a", "b"]), fingerprint(&["b", "a"]));
    }

    #[test]
    fn fingerprint_does_not_collide_on_field_boundaries() {
        // The classic concatenation bug: without length prefixes these match.
        assert_ne!(fingerprint(&["ab", "c"]), fingerprint(&["a", "bc"]));
        assert_ne!(fingerprint(&["", "ab"]), fingerprint(&["ab", ""]));
    }
}
