//! Application state for the TUI.

use std::collections::HashMap;
use std::time::Instant;

use crate::actions::{Action, Command, Direction};

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
pub struct MilestoneOverlayState { pub items: Vec<MilestoneRef>, pub cursor: usize, pub query: String, pub show_all: bool }

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

    pub fn remember_cursor(&mut self) { self.last_card_idx_per_column.insert(self.focus.column, self.focus.card_idx); }
    pub fn restore_cursor_for(&self, col: ColumnId) -> usize {
        let len = self.column_cards(col).len();
        if len == 0 { return 0; }
        self.last_card_idx_per_column.get(&col).copied().unwrap_or(0).min(len - 1)
    }
    pub fn auto_focus_if_empty(&mut self) {
        if self.focused_card().is_some() { return; }
        for col in self.visible_columns() {
            if !self.column_cards(col).is_empty() {
                self.focus.column = col; self.focus.card_idx = self.restore_cursor_for(col); return;
            }
        }
        self.focus.card_idx = 0;
    }
}

impl Default for App { fn default() -> Self { Self::new() } }

pub fn update(app: &mut App, action: Action) -> Option<Command> {
    match action {
        Action::FocusMove(d) => { move_focus(app, d); None }
        Action::JumpToTop => { app.focus.card_idx = 0; None }
        Action::JumpToBottom => { app.focus.card_idx = app.column_cards(app.focus.column).len().saturating_sub(1); None }
        Action::ToggleHelp => { app.mode = match app.mode { Mode::Help => Mode::Normal, _ => Mode::Help }; None }
        Action::QuitRequest => { app.mode = Mode::ConfirmQuit; None }
        Action::Quit => None,
        Action::Cancel => { app.mode = Mode::Normal; None }
        Action::Refresh => Some(Command::Reload),
        Action::OpenDetail => { if let Some(c) = app.focused_card() { app.mode = Mode::Detail(c.key.clone()); } None }
        Action::BeginMove => { app.mode = Mode::AwaitingMove; None }
        Action::MoveTo(status) => { app.mode = Mode::Normal; let key = app.focused_card()?.key.clone(); Some(Command::MoveIssue { key, status }) }
        Action::MoveIssueDir(d) => {
            let key = app.focused_card()?.key.clone();
            match d {
                Direction::Left | Direction::Right => {
                    let cur = ColumnId::ALL.iter().position(|c| *c == app.focus.column)?;
                    let target_idx = match d {
                        Direction::Left => cur.checked_sub(1)?,
                        _ => { let n = cur + 1; if n >= ColumnId::ALL.len() { return None; } n }
                    };
                    let target = ColumnId::ALL[target_idx];
                    let status = target.status().to_string();
                    // core's move_issue appends to the end of the target column, so the
                    // cursor follows the card to its landing slot there.
                    let landing = app.column_cards(target).len();
                    app.focus.column = target; app.focus.card_idx = landing;
                    Some(Command::MoveIssue { key, status })
                }
                Direction::Down | Direction::Up => {
                    let cards = app.column_cards(app.focus.column);
                    let idx = app.focus.card_idx;
                    let other_idx = match d {
                        Direction::Down => { let n = idx + 1; if n >= cards.len() { return None; } n }
                        _ => idx.checked_sub(1)?,
                    };
                    let other = cards[other_idx].key.clone();
                    app.focus.card_idx = other_idx; // cursor follows the reordered card
                    Some(Command::Reorder { key, other })
                }
            }
        }
        Action::Archive => { let key = app.focused_card()?.key.clone(); Some(Command::Archive { key }) }
        Action::EditCard => { let key = app.focused_card()?.key.clone(); Some(Command::EditIssue { key }) }
        Action::EditScope => match (&app.scope.project, &app.scope.milestone) {
            (Some(_), Some(m)) => Some(Command::EditMilestone { name: m.clone() }),
            (Some(_), None) => Some(Command::EditProject),
            (None, _) => { app.status_msg = Some("scope a project first (p) to edit it".into()); None }
        },
        Action::NewIssue => { let status = app.focus.column.status().to_string(); Some(Command::NewIssue { status }) }
        Action::NewMilestone => Some(Command::NewMilestone),
        Action::TagMilestone => {
            let card = app.focused_card()?.clone();
            if app.milestones.is_empty() { app.status_msg = Some("no milestones to tag (N to create one)".into()); return None; }
            let cur = card.milestone_id
                .and_then(|id| app.milestones.iter().position(|m| m.id == id))
                .map(|i| i + 1).unwrap_or(0);
            let next = (cur + 1) % (app.milestones.len() + 1);
            let milestone = if next == 0 { None } else { Some(app.milestones[next - 1].name.clone()) };
            Some(Command::TagMilestone { key: card.key, milestone })
        }
        _ => update_overlays(app, action),
    }
}

fn update_overlays(app: &mut App, action: Action) -> Option<Command> {
    match action {
        Action::OpenProjectPicker => { app.mode = Mode::ProjectPicker(PickerState { query: String::new(), items: vec![], cursor: 0 }); None }
        Action::OpenMilestonePicker => {
            if app.scope.project.is_none() { app.status_msg = Some("scope a project first with p".into()); return None; }
            app.mode = Mode::MilestonePicker(PickerState { query: String::new(), items: vec![], cursor: 0 }); None
        }
        Action::OpenMilestoneOverlay => { app.mode = Mode::MilestoneOverlay(MilestoneOverlayState { items: app.milestones.clone(), cursor: 0, query: String::new(), show_all: false }); None }
        Action::CycleMilestoneFilter => {
            if app.milestones.is_empty() { return None; }
            let names: Vec<&str> = app.milestones.iter().map(|m| m.name.as_str()).collect();
            let next = match &app.scope.milestone {
                None => Some(names[0].to_string()),
                Some(cur) => match names.iter().position(|n| *n == cur.as_str()) {
                    Some(i) if i + 1 < names.len() => Some(names[i + 1].to_string()),
                    _ => None,
                },
            };
            app.scope.milestone = next; app.auto_focus_if_empty(); Some(Command::SetScope)
        }
        Action::SetScope(s) => { app.scope = s; app.auto_focus_if_empty(); Some(Command::SetScope) }
        Action::PickerInput(c) => { with_picker(app, |p| { p.query.push(c); p.cursor = 0; }); None }
        Action::PickerBackspace => { with_picker(app, |p| { p.query.pop(); p.cursor = 0; }); None }
        Action::PickerUp => { with_picker(app, |p| p.cursor = p.cursor.saturating_sub(1)); None }
        Action::PickerDown => { with_picker(app, |p| { let n = filtered_picker(p).len(); if p.cursor + 1 < n { p.cursor += 1; } }); None }
        Action::PickerConfirm => picker_confirm(app),
        _ => update_fuzzy_overlay(app, action),
    }
}

fn with_picker(app: &mut App, f: impl FnOnce(&mut PickerState)) {
    match &mut app.mode { Mode::ProjectPicker(p) | Mode::MilestonePicker(p) => f(p), _ => {} }
}

/// Substring filter over picker labels; returns indices into `items`.
pub fn filtered_picker(p: &PickerState) -> Vec<usize> {
    if p.query.is_empty() { return (0..p.items.len()).collect(); }
    let q = p.query.to_lowercase();
    p.items.iter().enumerate().filter(|(_, c)| c.label.to_lowercase().contains(&q)).map(|(i, _)| i).collect()
}

/// Case-insensitive substring filter over milestone names; returns indices
/// into the overlay's `items`. Empty query matches everything. Mirrors
/// `filtered_picker` so the milestone overlay filters like the project picker.
pub fn filtered_overlay(o: &MilestoneOverlayState) -> Vec<usize> {
    // Two conditions: unless `show_all`, keep only open milestones; and the name
    // must contain the (case-insensitive) query. Empty query matches every name.
    let q = o.query.to_lowercase();
    o.items.iter().enumerate()
        .filter(|(_, m)| o.show_all || m.status == "open")
        .filter(|(_, m)| q.is_empty() || m.name.to_lowercase().contains(&q))
        .map(|(i, _)| i).collect()
}

fn picker_confirm(app: &mut App) -> Option<Command> {
    let (is_project, chip) = match &app.mode {
        Mode::ProjectPicker(p) => { let i = *filtered_picker(p).get(p.cursor)?; (true, p.items[i].clone()) }
        Mode::MilestonePicker(p) => { let i = *filtered_picker(p).get(p.cursor)?; (false, p.items[i].clone()) }
        _ => return None,
    };
    app.mode = Mode::Normal;
    if is_project { app.scope.set_project(Some(chip.value)); } else { app.scope.milestone = Some(chip.value); }
    app.auto_focus_if_empty();
    Some(Command::SetScope)
}

fn update_fuzzy_overlay(app: &mut App, action: Action) -> Option<Command> {
    match action {
        Action::OpenFuzzyFind => { let results = fuzzy_search(app, ""); app.mode = Mode::FuzzyFind(FuzzyState { query: String::new(), results, cursor: 0 }); None }
        Action::FuzzyInput(c) => {
            let q = match &app.mode { Mode::FuzzyFind(f) => { let mut q = f.query.clone(); q.push(c); q } _ => return None };
            let r = fuzzy_search(app, &q);
            if let Mode::FuzzyFind(f) = &mut app.mode { f.query = q; f.results = r; f.cursor = 0; } None
        }
        Action::FuzzyBackspace => {
            let q = match &app.mode { Mode::FuzzyFind(f) => { let mut q = f.query.clone(); q.pop(); q } _ => return None };
            let r = fuzzy_search(app, &q);
            if let Mode::FuzzyFind(f) = &mut app.mode { f.query = q; f.results = r; f.cursor = 0; } None
        }
        Action::FuzzyUp => { if let Mode::FuzzyFind(f) = &mut app.mode { f.cursor = f.cursor.saturating_sub(1); } None }
        Action::FuzzyDown => { if let Mode::FuzzyFind(f) = &mut app.mode { let m = f.results.len().saturating_sub(1); if f.cursor < m { f.cursor += 1; } } None }
        Action::FuzzyConfirm => {
            let target = match &app.mode { Mode::FuzzyFind(f) => f.results.get(f.cursor).cloned()?, _ => return None };
            if let Some(focus) = locate_focus_for_key(app, &target) { app.focus = focus; }
            app.mode = Mode::Normal; None
        }
        Action::OverlayInput(c) => { if let Mode::MilestoneOverlay(o) = &mut app.mode { o.query.push(c); o.cursor = 0; } None }
        Action::OverlayBackspace => { if let Mode::MilestoneOverlay(o) = &mut app.mode { o.query.pop(); o.cursor = 0; } None }
        Action::OverlayUp => { if let Mode::MilestoneOverlay(o) = &mut app.mode { o.cursor = o.cursor.saturating_sub(1); } None }
        Action::OverlayDown => { if let Mode::MilestoneOverlay(o) = &mut app.mode { let m = filtered_overlay(o).len().saturating_sub(1); if o.cursor < m { o.cursor += 1; } } None }
        Action::OverlayEdit => { let name = match &app.mode { Mode::MilestoneOverlay(o) => o.items.get(*filtered_overlay(o).get(o.cursor)?)?.name.clone(), _ => return None }; Some(Command::EditMilestone { name }) }
        Action::OverlaySelect => {
            let name = match &app.mode { Mode::MilestoneOverlay(o) => o.items.get(*filtered_overlay(o).get(o.cursor)?)?.name.clone(), _ => return None };
            app.scope.milestone = Some(name); app.mode = Mode::Normal; app.auto_focus_if_empty(); Some(Command::SetScope)
        }
        Action::OverlayToggleAll => { if let Mode::MilestoneOverlay(o) = &mut app.mode { o.show_all = !o.show_all; o.cursor = 0; } None }
        _ => None,
    }
}

pub fn fuzzy_search(app: &App, query: &str) -> Vec<String> {
    let needle = query.to_lowercase();
    let mut out = Vec::new();
    for col in app.visible_columns() {
        for c in app.column_cards(col) {
            let hay = format!("{} {}", c.key, c.title).to_lowercase();
            if needle.is_empty() || hay.contains(&needle) { out.push(c.key.clone()); }
        }
    }
    out
}

fn locate_focus_for_key(app: &App, key: &str) -> Option<Focus> {
    for col in app.visible_columns() {
        for (idx, c) in app.column_cards(col).iter().enumerate() {
            if c.key == key { return Some(Focus { column: col, card_idx: idx }); }
        }
    }
    None
}

fn move_focus(app: &mut App, dir: Direction) {
    let columns = app.visible_columns();
    let cur = columns.iter().position(|c| *c == app.focus.column).unwrap_or(0);
    match dir {
        Direction::Left => if let Some(t) = (0..cur).rev().find(|&i| !app.column_cards(columns[i]).is_empty()) {
            app.remember_cursor(); app.focus.column = columns[t]; app.focus.card_idx = app.restore_cursor_for(columns[t]); },
        Direction::Right => if let Some(t) = (cur + 1..columns.len()).find(|&i| !app.column_cards(columns[i]).is_empty()) {
            app.remember_cursor(); app.focus.column = columns[t]; app.focus.card_idx = app.restore_cursor_for(columns[t]); },
        Direction::Up => if app.focus.card_idx > 0 { app.focus.card_idx -= 1; },
        Direction::Down => { let n = app.column_cards(app.focus.column).len(); if app.focus.card_idx + 1 < n { app.focus.card_idx += 1; } }
    }
}

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

    use crate::actions::{Action, Command, Direction};

    #[test]
    fn space_then_letter_moves_focused_card() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1","backlog",1000.0)]; app.auto_focus_if_empty();
        assert!(update(&mut app, Action::BeginMove).is_none());
        assert!(matches!(app.mode, Mode::AwaitingMove));
        match update(&mut app, Action::MoveTo("in-progress".into())).unwrap() {
            Command::MoveIssue { key, status } => { assert_eq!(key, "CLI-1"); assert_eq!(status, "in-progress"); } _ => panic!() }
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn capital_l_moves_issue_to_next_column_and_cursor_follows() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1","backlog",1000.0)]; app.auto_focus_if_empty();
        match update(&mut app, Action::MoveIssueDir(Direction::Right)).unwrap() {
            Command::MoveIssue { key, status } => { assert_eq!(key, "CLI-1"); assert_eq!(status, "in-progress"); }
            _ => panic!(),
        }
        assert_eq!(app.focus.column, ColumnId::InProgress);
        assert_eq!(app.focus.card_idx, 0); // landing slot in the (was empty) target column
    }

    #[test]
    fn capital_h_at_leftmost_column_is_noop() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1","backlog",1000.0)]; app.auto_focus_if_empty();
        assert!(update(&mut app, Action::MoveIssueDir(Direction::Left)).is_none());
        assert_eq!(app.focus.column, ColumnId::Backlog);
    }

    #[test]
    fn capital_j_reorders_within_column_and_cursor_follows() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1","backlog",1000.0), card("CLI-2","backlog",2000.0)];
        app.scope.set_project(Some("CLI".into())); app.auto_focus_if_empty();
        assert_eq!(app.focus.card_idx, 0); // on CLI-1
        match update(&mut app, Action::MoveIssueDir(Direction::Down)).unwrap() {
            Command::Reorder { key, other } => { assert_eq!(key, "CLI-1"); assert_eq!(other, "CLI-2"); }
            _ => panic!(),
        }
        assert_eq!(app.focus.card_idx, 1); // cursor follows the card down
    }

    #[test]
    fn capital_j_at_bottom_of_column_is_noop() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1","backlog",1000.0)];
        app.scope.set_project(Some("CLI".into())); app.auto_focus_if_empty();
        assert!(update(&mut app, Action::MoveIssueDir(Direction::Down)).is_none());
    }

    #[test]
    fn tag_milestone_cycles_none_to_first() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1","backlog",1000.0)];
        app.milestones = vec![MilestoneRef { id: 7, name: "M1".into(), status: "open".into(), target: None }];
        app.auto_focus_if_empty();
        match update(&mut app, Action::TagMilestone).unwrap() {
            Command::TagMilestone { key, milestone } => { assert_eq!(key, "CLI-1"); assert_eq!(milestone, Some("M1".to_string())); } _ => panic!() }
    }

    #[test]
    fn fuzzy_search_matches_key_and_title() {
        let mut app = App::new();
        let mut c = card("CLI-1","backlog",1.0); c.title = "Build the board".into();
        app.cards = vec![c];
        assert_eq!(fuzzy_search(&app, "board"), vec!["CLI-1"]);
        assert_eq!(fuzzy_search(&app, "cli-1"), vec!["CLI-1"]);
        assert!(fuzzy_search(&app, "zzz").is_empty());
    }

    #[test]
    fn focus_move_right_skips_empty_columns() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1","blocked",1.0)];
        app.focus = Focus { column: ColumnId::Backlog, card_idx: 0 };
        update(&mut app, Action::FocusMove(Direction::Right));
        assert_eq!(app.focus.column, ColumnId::Blocked);
    }

    #[test]
    fn cycle_milestone_filter_advances_then_wraps_to_all() {
        let mut app = App::new();
        app.milestones = vec![
            MilestoneRef { id:1, name:"M1".into(), status:"open".into(), target:None },
            MilestoneRef { id:2, name:"M2".into(), status:"open".into(), target:None },
        ];
        update(&mut app, Action::CycleMilestoneFilter); assert_eq!(app.scope.milestone.as_deref(), Some("M1"));
        update(&mut app, Action::CycleMilestoneFilter); assert_eq!(app.scope.milestone.as_deref(), Some("M2"));
        update(&mut app, Action::CycleMilestoneFilter); assert_eq!(app.scope.milestone, None);
    }

    #[test]
    fn overlay_query_filters_and_select_resolves_against_filtered_list() {
        let mut app = App::new();
        app.milestones = vec![
            MilestoneRef { id:1, name:"alpha".into(), status:"open".into(), target:None },
            MilestoneRef { id:2, name:"beta".into(), status:"open".into(), target:None },
            MilestoneRef { id:3, name:"gamma".into(), status:"open".into(), target:None },
        ];
        update(&mut app, Action::OpenMilestoneOverlay);
        // Typing narrows the list; only "beta" matches "be".
        update(&mut app, Action::OverlayInput('b'));
        update(&mut app, Action::OverlayInput('e'));
        match &app.mode {
            Mode::MilestoneOverlay(o) => {
                assert_eq!(o.query, "be");
                assert_eq!(o.cursor, 0, "typing resets the cursor to the top");
                assert_eq!(filtered_overlay(o), vec![1]);
            }
            _ => panic!("expected overlay mode"),
        }
        // Enter on the single match sets that milestone, not items[cursor].
        let cmd = update(&mut app, Action::OverlaySelect);
        assert_eq!(app.scope.milestone.as_deref(), Some("beta"));
        assert!(matches!(cmd, Some(Command::SetScope)));
    }

    #[test]
    fn overlay_down_is_clamped_to_filtered_len() {
        let mut app = App::new();
        app.milestones = vec![
            MilestoneRef { id:1, name:"alpha".into(), status:"open".into(), target:None },
            MilestoneRef { id:2, name:"beta".into(), status:"open".into(), target:None },
        ];
        update(&mut app, Action::OpenMilestoneOverlay);
        update(&mut app, Action::OverlayInput('a')); // matches "alpha" and "beta"? "a" is in both
        update(&mut app, Action::OverlayInput('l')); // "al" -> only "alpha"
        update(&mut app, Action::OverlayDown); // one match: cursor must stay at 0
        match &app.mode { Mode::MilestoneOverlay(o) => assert_eq!(o.cursor, 0), _ => panic!() }
    }

    #[test]
    fn overlay_backspace_widens_the_filter() {
        let mut app = App::new();
        app.milestones = vec![
            MilestoneRef { id:1, name:"alpha".into(), status:"open".into(), target:None },
            MilestoneRef { id:2, name:"beta".into(), status:"open".into(), target:None },
        ];
        update(&mut app, Action::OpenMilestoneOverlay);
        update(&mut app, Action::OverlayInput('b'));
        update(&mut app, Action::OverlayBackspace);
        match &app.mode {
            Mode::MilestoneOverlay(o) => { assert_eq!(o.query, ""); assert_eq!(filtered_overlay(o).len(), 2); }
            _ => panic!(),
        }
    }

    #[test]
    fn overlay_enter_sets_milestone_filter_and_closes() {
        let mut app = App::new();
        app.milestones = vec![
            MilestoneRef { id:1, name:"M1".into(), status:"open".into(), target:None },
            MilestoneRef { id:2, name:"M2".into(), status:"open".into(), target:None },
        ];
        update(&mut app, Action::OpenMilestoneOverlay);
        update(&mut app, Action::OverlayDown); // cursor -> M2
        let cmd = update(&mut app, Action::OverlaySelect);
        assert_eq!(app.scope.milestone.as_deref(), Some("M2"));
        assert!(matches!(app.mode, Mode::Normal), "overlay should close after select");
        assert!(matches!(cmd, Some(Command::SetScope)));
    }

    #[test]
    fn overlay_opens_showing_open_milestones_only() {
        let mut app = App::new();
        app.milestones = vec![
            MilestoneRef { id:1, name:"open-one".into(),  status:"open".into(),      target:None },
            MilestoneRef { id:2, name:"done-one".into(),  status:"completed".into(), target:None },
            MilestoneRef { id:3, name:"axed-one".into(),  status:"cancelled".into(), target:None },
        ];
        update(&mut app, Action::OpenMilestoneOverlay);
        match &app.mode {
            // only the open milestone (items index 0) is visible by default
            Mode::MilestoneOverlay(o) => { assert!(!o.show_all); assert_eq!(filtered_overlay(o), vec![0]); }
            _ => panic!("expected overlay mode"),
        }
    }

    #[test]
    fn overlay_toggle_all_reveals_then_hides_non_open() {
        let mut app = App::new();
        app.milestones = vec![
            MilestoneRef { id:1, name:"open-one".into(),  status:"open".into(),      target:None },
            MilestoneRef { id:2, name:"done-one".into(),  status:"completed".into(), target:None },
            MilestoneRef { id:3, name:"axed-one".into(),  status:"cancelled".into(), target:None },
        ];
        update(&mut app, Action::OpenMilestoneOverlay);
        update(&mut app, Action::OverlayToggleAll); // reveal all statuses
        match &app.mode {
            Mode::MilestoneOverlay(o) => { assert!(o.show_all); assert_eq!(filtered_overlay(o), vec![0,1,2]); }
            _ => panic!("expected overlay mode"),
        }
        update(&mut app, Action::OverlayToggleAll); // back to open-only
        match &app.mode {
            Mode::MilestoneOverlay(o) => { assert!(!o.show_all); assert_eq!(filtered_overlay(o), vec![0]); }
            _ => panic!("expected overlay mode"),
        }
    }
}
