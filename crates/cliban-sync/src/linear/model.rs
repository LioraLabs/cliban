//! Serde shapes for the slice of Linear's GraphQL schema cliban touches.
//!
//! Deliberately partial. Every struct here mirrors exactly the fields the
//! queries in [`super::ops`] request, so a field appearing here without a
//! matching selection in the query is a deserialization failure waiting to
//! happen, not dead code.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

/// Linear wraps every collection in `{ "nodes": [...] }`.
#[derive(Debug, Clone, Deserialize)]
pub struct Nodes<T> {
    pub nodes: Vec<T>,
}

impl<T> Nodes<T> {
    pub fn first(self) -> Option<T> {
        self.nodes.into_iter().next()
    }
}

/// A workflow state (Linear's per-team columns).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WorkflowState {
    pub id: String,
    pub name: String,
    /// One of `triage`, `backlog`, `unstarted`, `started`, `completed`,
    /// `canceled`. Free-form in the API; unknown values fall through to the
    /// default arm of the status map rather than failing the import.
    #[serde(rename = "type")]
    pub kind: String,
    /// Order within the team's board. Used to break ties when several states
    /// share a type — the leftmost is the one a human would consider canonical.
    #[serde(default)]
    pub position: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TeamRef {
    pub id: String,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Team {
    pub id: String,
    pub key: String,
    pub name: String,
    pub states: Nodes<WorkflowState>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct LabelRef {
    pub name: String,
}

/// One Linear issue, as much of it as cliban mirrors.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// The stable UUID. This is what mutations address.
    pub id: String,
    /// The human key, e.g. `ENG-412`.
    pub identifier: String,
    pub title: String,
    /// Linear allows an empty description; it comes back as null, not "".
    #[serde(default)]
    pub description: Option<String>,
    pub url: String,
    pub updated_at: DateTime<Utc>,
    /// 0 none, 1 urgent, 2 high, 3 medium, 4 low. A float in the schema.
    #[serde(default)]
    pub priority: f64,
    #[serde(default)]
    pub due_date: Option<NaiveDate>,
    pub state: WorkflowState,
    pub team: TeamRef,
    #[serde(default = "empty_labels")]
    pub labels: Nodes<LabelRef>,
}

fn empty_labels() -> Nodes<LabelRef> {
    Nodes { nodes: Vec::new() }
}

impl Issue {
    /// Label names in the order Linear returned them.
    pub fn label_names(&self) -> Vec<String> {
        self.labels.nodes.iter().map(|l| l.name.clone()).collect()
    }

    /// Description with `None` and `Some("")` collapsed — every consumer wants
    /// the same thing from both.
    pub fn description_text(&self) -> &str {
        self.description.as_deref().unwrap_or("").trim_end()
    }
}

/// `priority` as cliban spells it. Linear's scale runs *downward* from urgent,
/// with 0 meaning "no priority" rather than "lowest", so this is not a plain
/// numeric ordering.
pub fn priority_to_cliban(linear: f64) -> &'static str {
    match linear.round() as i64 {
        1 => "urgent",
        2 => "high",
        3 => "medium",
        4 => "low",
        // 0 is "No priority"; anything else is a scale Linear grew after this
        // was written, and "none" is the honest answer for an unknown level.
        _ => "none",
    }
}

/// The inverse of [`priority_to_cliban`].
pub fn priority_from_cliban(status: &str) -> f64 {
    match status {
        "urgent" => 1.0,
        "high" => 2.0,
        "medium" => 3.0,
        "low" => 4.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_round_trips_through_cliban_names() {
        for name in ["none", "low", "medium", "high", "urgent"] {
            let back = priority_to_cliban(priority_from_cliban(name));
            assert_eq!(back, name, "{name} did not survive the round trip");
        }
    }

    #[test]
    fn priority_zero_is_none_not_lowest() {
        // The trap: 0 sorts lowest numerically but means "unset" in Linear.
        assert_eq!(priority_to_cliban(0.0), "none");
        assert_eq!(priority_to_cliban(4.0), "low");
    }

    #[test]
    fn unknown_priority_level_degrades_to_none() {
        assert_eq!(priority_to_cliban(9.0), "none");
        assert_eq!(priority_from_cliban("catastrophic"), 0.0);
    }

    #[test]
    fn issue_parses_with_null_description_and_no_labels() {
        // Both are what a freshly-created Linear issue actually returns.
        let json = serde_json::json!({
            "id": "uuid-1",
            "identifier": "ENG-412",
            "title": "Fix the thing",
            "description": null,
            "url": "https://linear.app/acme/issue/ENG-412",
            "updatedAt": "2026-07-29T12:00:00.000Z",
            "priority": 0,
            "dueDate": null,
            "state": {"id": "s1", "name": "Todo", "type": "unstarted", "position": 1.0},
            "team": {"id": "t1", "key": "ENG", "name": "Engineering"},
            "labels": {"nodes": []}
        });
        let issue: Issue = serde_json::from_value(json).unwrap();
        assert_eq!(issue.identifier, "ENG-412");
        assert_eq!(issue.description_text(), "");
        assert!(issue.label_names().is_empty());
        assert_eq!(issue.due_date, None);
    }

    #[test]
    fn issue_parses_dates_labels_and_priority() {
        let json = serde_json::json!({
            "id": "uuid-2",
            "identifier": "ENG-9",
            "title": "Ship it",
            "description": "Some **markdown**.\n",
            "url": "https://linear.app/acme/issue/ENG-9",
            "updatedAt": "2026-07-29T12:00:00.000Z",
            "priority": 2,
            "dueDate": "2026-08-15",
            "state": {"id": "s2", "name": "In Review", "type": "started", "position": 3.0},
            "team": {"id": "t1", "key": "ENG", "name": "Engineering"},
            "labels": {"nodes": [{"name": "bug"}, {"name": "p0"}]}
        });
        let issue: Issue = serde_json::from_value(json).unwrap();
        assert_eq!(issue.label_names(), vec!["bug", "p0"]);
        assert_eq!(
            issue.due_date,
            Some(NaiveDate::from_ymd_opt(2026, 8, 15).unwrap())
        );
        assert_eq!(priority_to_cliban(issue.priority), "high");
        assert_eq!(issue.description_text(), "Some **markdown**.");
    }
}
