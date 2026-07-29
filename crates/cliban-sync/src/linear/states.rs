//! Mapping cliban's five fixed statuses onto a Linear team's arbitrary
//! workflow states, and back.
//!
//! This is the one genuinely lossy part of the bridge. cliban has a closed
//! vocabulary (`backlog`, `in-progress`, `blocked`, `in-review`, `done`);
//! a Linear team can name its columns anything and have any number of them.
//! What makes it work without configuration is that every Linear state also
//! carries a *type* from a closed set, so there is always a defensible answer
//! even when no name matches.
//!
//! Resolution order, both directions:
//!
//! 1. **Name.** Normalized comparison, so `"In Progress"`, `"in-progress"` and
//!    `"In  Progress"` all match cliban's `in-progress`. This is what makes the
//!    common case need no config — most teams call their columns roughly what
//!    cliban calls them.
//! 2. **Type.** `triage`/`backlog`/`unstarted` → `backlog`, `started` →
//!    `in-progress`, `completed` → `done`, `canceled` → `done` (archived).
//!
//! `blocked` and `in-review` have no type of their own — Linear models both as
//! `started` — so they survive a round trip only when the team has a column
//! named for them, or an override in `linear.toml`. That is a real limitation
//! and the config file exists because of it.

use std::collections::BTreeMap;

use super::model::WorkflowState;

/// What a Linear state means on a cliban board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapped {
    pub status: &'static str,
    /// Linear's `canceled` has no cliban status: the work is over but it was
    /// not completed. `done` + archived is the closest honest rendering, and
    /// it keeps cancelled issues off the board without deleting anything.
    pub archive: bool,
}

/// Normalize a state name for comparison: lowercase, runs of non-alphanumerics
/// collapsed to a single `-`, ends trimmed. `"In  Review!"` → `"in-review"`.
pub fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_sep = false;
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_sep = true;
        }
    }
    out
}

/// Linear workflow state → cliban status.
pub fn to_cliban(state: &WorkflowState) -> Mapped {
    // Name first: a column literally called "Blocked" or "In Review" carries
    // more information than its type does, since Linear types both as
    // "started".
    let normalized = normalize(&state.name);
    if cliban_core::schema::ISSUE_STATUSES.contains(&normalized.as_str()) {
        return Mapped {
            status: canonical(&normalized),
            archive: false,
        };
    }
    // A few spellings that are common enough to be worth knowing about.
    if let Some(status) = alias(&normalized) {
        return Mapped {
            status,
            archive: false,
        };
    }
    match state.kind.as_str() {
        "started" => Mapped {
            status: "in-progress",
            archive: false,
        },
        "completed" => Mapped {
            status: "done",
            archive: false,
        },
        "canceled" | "cancelled" => Mapped {
            status: "done",
            archive: true,
        },
        // triage, backlog, unstarted, and anything Linear adds later.
        _ => Mapped {
            status: "backlog",
            archive: false,
        },
    }
}

/// cliban status → the best Linear state among a team's `states`.
///
/// `overrides` is `[linear.states]` from the config file: a cliban status
/// mapped to an exact Linear state name. An override naming a state the team
/// does not have is ignored rather than fatal — teams get reorganized, and
/// refusing to push because of a stale config line would be worse than falling
/// back to inference.
pub fn to_linear<'a>(
    status: &str,
    states: &'a [WorkflowState],
    overrides: &BTreeMap<String, String>,
) -> Option<&'a WorkflowState> {
    if let Some(want) = overrides.get(status) {
        let want_norm = normalize(want);
        if let Some(found) = states.iter().find(|s| normalize(&s.name) == want_norm) {
            return Some(found);
        }
    }

    // Exact name match, e.g. cliban "in-review" → a column called "In Review".
    if let Some(found) = states.iter().find(|s| normalize(&s.name) == status) {
        return Some(found);
    }
    if let Some(found) = states
        .iter()
        .find(|s| alias(&normalize(&s.name)) == Some(status))
    {
        return Some(found);
    }

    // Fall back to type, taking the leftmost state of the right type so the
    // choice is stable and matches what a human reads as "the" Todo column.
    let wanted_kinds: &[&str] = match status {
        "backlog" => &["backlog", "triage", "unstarted"],
        "in-progress" | "blocked" | "in-review" => &["started"],
        "done" => &["completed"],
        _ => &[],
    };
    for kind in wanted_kinds {
        let mut candidates: Vec<&WorkflowState> =
            states.iter().filter(|s| s.kind == *kind).collect();
        candidates.sort_by(|a, b| {
            a.position
                .partial_cmp(&b.position)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some(first) = candidates.first() {
            return Some(first);
        }
    }
    None
}

/// Borrow the `'static` spelling from the canonical list so [`Mapped`] can hold
/// `&'static str` rather than an allocation.
fn canonical(normalized: &str) -> &'static str {
    cliban_core::schema::ISSUE_STATUSES
        .iter()
        .copied()
        .find(|s| *s == normalized)
        .unwrap_or("backlog")
}

/// Common column names that mean a cliban status but do not spell it.
fn alias(normalized: &str) -> Option<&'static str> {
    match normalized {
        "todo" | "to-do" | "triage" | "unstarted" | "planned" => Some("backlog"),
        "doing" | "started" | "wip" | "in-development" | "in-dev" => Some("in-progress"),
        "review" | "code-review" | "in-code-review" | "reviewing" => Some("in-review"),
        "on-hold" | "paused" | "stalled" | "waiting" => Some("blocked"),
        "complete" | "completed" | "shipped" | "merged" | "closed" => Some("done"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str, kind: &str, position: f64) -> WorkflowState {
        WorkflowState {
            id: format!("id-{}", normalize(name)),
            name: name.into(),
            kind: kind.into(),
            position,
        }
    }

    fn team_states() -> Vec<WorkflowState> {
        vec![
            state("Triage", "triage", 0.0),
            state("Backlog", "backlog", 1.0),
            state("Todo", "unstarted", 2.0),
            state("In Progress", "started", 3.0),
            state("In Review", "started", 4.0),
            state("Done", "completed", 5.0),
            state("Canceled", "canceled", 6.0),
        ]
    }

    #[test]
    fn normalize_collapses_case_and_punctuation() {
        assert_eq!(normalize("In Progress"), "in-progress");
        assert_eq!(normalize("in-progress"), "in-progress");
        assert_eq!(normalize("In  Review!"), "in-review");
        assert_eq!(normalize("  Done  "), "done");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn name_beats_type_for_in_review() {
        // The whole reason name wins: Linear types "In Review" as `started`,
        // which would otherwise flatten it into in-progress.
        let mapped = to_cliban(&state("In Review", "started", 4.0));
        assert_eq!(mapped.status, "in-review");
        assert!(!mapped.archive);
    }

    #[test]
    fn name_beats_type_for_blocked() {
        let mapped = to_cliban(&state("Blocked", "started", 4.0));
        assert_eq!(mapped.status, "blocked");
    }

    #[test]
    fn type_carries_states_with_unrecognized_names() {
        assert_eq!(
            to_cliban(&state("Cooking", "started", 1.0)).status,
            "in-progress"
        );
        assert_eq!(
            to_cliban(&state("Icebox", "backlog", 1.0)).status,
            "backlog"
        );
        assert_eq!(
            to_cliban(&state("Shipped 🚀", "completed", 1.0)).status,
            "done"
        );
    }

    #[test]
    fn canceled_becomes_done_and_archived() {
        let mapped = to_cliban(&state("Canceled", "canceled", 6.0));
        assert_eq!(mapped.status, "done");
        assert!(mapped.archive, "cancelled work should leave the board");
    }

    #[test]
    fn an_unknown_type_degrades_to_backlog() {
        // Linear adding a new state type must not fail an import.
        assert_eq!(
            to_cliban(&state("Whatever", "quantum", 1.0)).status,
            "backlog"
        );
    }

    #[test]
    fn to_linear_prefers_the_matching_name() {
        let states = team_states();
        let empty = BTreeMap::new();
        let s = to_linear("in-review", &states, &empty).unwrap();
        assert_eq!(s.name, "In Review");
    }

    #[test]
    fn to_linear_falls_back_to_type_and_picks_the_leftmost() {
        // No column named "backlog"? Take the leftmost backlog-ish one.
        let states = vec![
            state("Icebox", "backlog", 5.0),
            state("Someday", "backlog", 2.0),
            state("Doing", "started", 9.0),
        ];
        let s = to_linear("backlog", &states, &BTreeMap::new()).unwrap();
        assert_eq!(s.name, "Someday", "position, not declaration order");
    }

    #[test]
    fn blocked_without_a_column_lands_in_a_started_state() {
        // The documented lossy case: no "Blocked" column means blocked and
        // in-progress are indistinguishable upstream.
        let states = vec![
            state("In Progress", "started", 1.0),
            state("Done", "completed", 2.0),
        ];
        let s = to_linear("blocked", &states, &BTreeMap::new()).unwrap();
        assert_eq!(s.name, "In Progress");
    }

    #[test]
    fn config_override_wins_over_inference() {
        let states = team_states();
        let mut overrides = BTreeMap::new();
        overrides.insert("in-review".to_string(), "Done".to_string());
        let s = to_linear("in-review", &states, &overrides).unwrap();
        assert_eq!(s.name, "Done");
    }

    #[test]
    fn a_stale_override_falls_back_rather_than_failing() {
        let states = team_states();
        let mut overrides = BTreeMap::new();
        overrides.insert("in-review".to_string(), "Column We Deleted".to_string());
        let s = to_linear("in-review", &states, &overrides).unwrap();
        assert_eq!(s.name, "In Review", "inference still applies");
    }

    #[test]
    fn to_linear_returns_none_when_nothing_fits() {
        let states = vec![state("Done", "completed", 1.0)];
        assert!(to_linear("backlog", &states, &BTreeMap::new()).is_none());
    }

    #[test]
    fn every_cliban_status_resolves_against_a_conventional_team() {
        let states = team_states();
        for status in cliban_core::schema::ISSUE_STATUSES {
            assert!(
                to_linear(status, &states, &BTreeMap::new()).is_some(),
                "{status} did not map"
            );
        }
    }

    #[test]
    fn round_trip_is_stable_for_the_statuses_linear_can_express() {
        let states = team_states();
        for status in ["backlog", "in-progress", "in-review", "done"] {
            let linear = to_linear(status, &states, &BTreeMap::new()).unwrap();
            assert_eq!(to_cliban(linear).status, status, "{status} did not survive");
        }
    }
}
