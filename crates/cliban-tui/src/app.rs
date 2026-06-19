//! Application state for the TUI.

use std::collections::HashMap;
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

#[derive(Debug, Clone)]
pub struct PickerState { pub query: String, pub items: Vec<PickerChip>, pub cursor: usize }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerChip { pub label: String, pub value: String }

#[derive(Debug, Clone)]
pub struct FuzzyState { pub query: String, pub results: Vec<String>, pub cursor: usize }

#[derive(Debug, Clone)]
pub struct MilestoneOverlayState { pub items: Vec<MilestoneRef>, pub cursor: usize }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneRef { pub id: i64, pub name: String, pub status: String, pub target: Option<String> }

#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    Help,
    ConfirmQuit,
    AwaitingMove,                       // Space pressed; waiting for column letter
    Detail(String),                     // focused card key
    ProjectPicker(PickerState),
    MilestonePicker(PickerState),
    FuzzyFind(FuzzyState),
    MilestoneOverlay(MilestoneOverlayState),
}

#[derive(Debug, Clone)]
pub struct App {
    pub cards: Vec<Card>,
    pub milestones: Vec<MilestoneRef>,  // for scope.project, name order
    pub focus: Focus,
    pub mode: Mode,
    pub scope: Scope,
    pub status_msg: Option<String>,
    pub last_card_idx_per_column: HashMap<ColumnId, usize>,
    pub pending_g: bool,
    pub boot_at: Instant,
}

impl App {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(), milestones: Vec::new(), focus: Focus::default(),
            mode: Mode::Normal, scope: Scope::default(), status_msg: None,
            last_card_idx_per_column: HashMap::new(), pending_g: false, boot_at: Instant::now(),
        }
    }

    pub fn matches_scope(&self, c: &Card) -> bool {
        match (&self.scope.project, &self.scope.milestone) {
            (None, _) => true,
            (Some(p), None) => &c.project == p,
            (Some(p), Some(m)) => &c.project == p && c.milestone.as_deref() == Some(m.as_str()),
        }
    }

    pub fn column_cards(&self, col: ColumnId) -> Vec<&Card> {
        let mut v: Vec<&Card> = self.cards.iter()
            .filter(|c| ColumnId::from_status(&c.status) == Some(col))
            .filter(|c| self.matches_scope(c)).collect();
        v.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    pub fn visible_columns(&self) -> Vec<ColumnId> { ColumnId::ALL.to_vec() }

    pub fn focused_card(&self) -> Option<&Card> {
        self.column_cards(self.focus.column).into_iter().nth(self.focus.card_idx)
    }

    pub fn blocked_count(&self) -> usize { self.column_cards(ColumnId::Blocked).len() }

    pub fn scoped_card_count(&self) -> usize {
        self.cards.iter().filter(|c| self.matches_scope(c)).count()
    }
}

impl Default for App { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;

    fn card(key: &str, status: &str, pos: f64) -> Card {
        let project = key.split('-').next().unwrap().to_string();
        Card { id: 0, key: key.into(), project, title: format!("T {key}"),
               status: status.into(), priority: "medium".into(), position: pos,
               milestone_id: None, milestone: None }
    }

    #[test]
    fn column_cards_sorted_and_scope_filtered() {
        let mut app = App::new();
        app.cards = vec![card("CLI-2","backlog",2000.0), card("CLI-1","backlog",1000.0), card("LM-1","backlog",500.0)];
        let backlog = app.column_cards(ColumnId::Backlog);
        assert_eq!(backlog.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(), vec!["LM-1","CLI-1","CLI-2"]);
        app.scope.set_project(Some("CLI".into()));
        let scoped = app.column_cards(ColumnId::Backlog);
        assert_eq!(scoped.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(), vec!["CLI-1","CLI-2"]);
    }

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
