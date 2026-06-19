//! Application state for the TUI.

#[allow(unused_imports)]
use std::collections::HashMap;
#[allow(unused_imports)]
use std::time::Instant;

/// Display projection of a `cliban_core` issue. Pure data.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: i64,
    pub key: String,
    pub project: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub position: f64,
    pub milestone_id: Option<i64>,
    pub milestone: Option<String>,
}

/// cliban's 5 kanban columns (NOT loom's agent states).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnId { Backlog, InProgress, Blocked, InReview, Done }

impl ColumnId {
    pub const ALL: &'static [Self] =
        &[Self::Backlog, Self::InProgress, Self::Blocked, Self::InReview, Self::Done];

    pub fn from_status(s: &str) -> Option<Self> {
        match s {
            "backlog" => Some(Self::Backlog),
            "in-progress" => Some(Self::InProgress),
            "blocked" => Some(Self::Blocked),
            "in-review" => Some(Self::InReview),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
    pub fn status(&self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::InProgress => "in-progress",
            Self::Blocked => "blocked",
            Self::InReview => "in-review",
            Self::Done => "done",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Backlog => "BACKLOG",
            Self::InProgress => "IN-PROGRESS",
            Self::Blocked => "BLOCKED",
            Self::InReview => "IN-REVIEW",
            Self::Done => "DONE",
        }
    }
}

/// Scope chips (loom §18): project key + milestone name. Invariant: milestone
/// is None whenever project is None.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope { pub project: Option<String>, pub milestone: Option<String> }

impl Scope {
    pub fn set_project(&mut self, p: Option<String>) {
        self.project = p;
        if self.project.is_none() { self.milestone = None; }
    }
}

#[derive(Debug, Clone)]
pub struct Focus { pub column: ColumnId, pub card_idx: usize }
impl Default for Focus {
    fn default() -> Self { Self { column: ColumnId::Backlog, card_idx: 0 } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_status_round_trips() {
        for c in ColumnId::ALL { assert_eq!(ColumnId::from_status(c.status()), Some(*c)); }
        assert_eq!(ColumnId::from_status("nope"), None);
    }

    #[test]
    fn scope_clearing_project_clears_milestone() {
        let mut s = Scope { project: Some("CLI".into()), milestone: Some("M1".into()) };
        s.set_project(None);
        assert_eq!(s.milestone, None);
    }
}
