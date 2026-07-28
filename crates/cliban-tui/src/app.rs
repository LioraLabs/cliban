//! Application state for the TUI.

use std::collections::HashMap;
use std::time::Instant;

use chrono::{DateTime, Utc};

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
    /// Label names, name-ordered (as `issues::label_names` returns them).
    pub labels: Vec<String>,
}

/// cliban's 5 kanban columns (NOT loom's agent states).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnId {
    Backlog,
    InProgress,
    Blocked,
    InReview,
    Done,
}

impl ColumnId {
    pub const ALL: &'static [Self] = &[
        Self::Backlog,
        Self::InProgress,
        Self::Blocked,
        Self::InReview,
        Self::Done,
    ];

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
pub struct Scope {
    pub project: Option<String>,
    pub milestone: Option<String>,
}

impl Scope {
    pub fn set_project(&mut self, p: Option<String>) {
        self.project = p;
        if self.project.is_none() {
            self.milestone = None;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Focus {
    pub column: ColumnId,
    pub card_idx: usize,
}
impl Default for Focus {
    fn default() -> Self {
        Self {
            column: ColumnId::Backlog,
            card_idx: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PickerState {
    pub query: String,
    pub items: Vec<PickerChip>,
    pub cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerChip {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct FuzzyState {
    pub query: String,
    pub results: Vec<String>,
    pub cursor: usize,
}

/// The milestone page's view state. Deliberately holds no milestone data of
/// its own — rows are projected from `app.milestones` by [`page_rows`], so a
/// reload after an edit refreshes the page in place.
#[derive(Debug, Clone, Default)]
pub struct MilestonePageState {
    pub cursor: usize,
    pub query: String,
    pub filter: StatusFilter,
    pub sort: PageSort,
}

/// Which statuses the page lists. `archived` is not a separate bucket:
/// cancelled *is* the archived state for a milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusFilter {
    #[default]
    Open,
    Completed,
    Cancelled,
    All,
}

impl StatusFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::All => "all",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Open => Self::Completed,
            Self::Completed => Self::Cancelled,
            Self::Cancelled => Self::All,
            Self::All => Self::Open,
        }
    }
    fn prev(self) -> Self {
        match self {
            Self::Open => Self::All,
            Self::Completed => Self::Open,
            Self::Cancelled => Self::Completed,
            Self::All => Self::Cancelled,
        }
    }
    fn accepts(self, status: &str) -> bool {
        match self {
            Self::All => true,
            Self::Open => status == "open",
            Self::Completed => status == "completed",
            Self::Cancelled => status == "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSort {
    /// Most recently worked on first — how `data::load_milestones` already
    /// orders the rows, so this sort is the identity.
    #[default]
    Activity,
    Name,
    Target,
}

impl PageSort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Activity => "activity",
            Self::Name => "name",
            Self::Target => "target",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Activity => Self::Name,
            Self::Name => Self::Target,
            Self::Target => Self::Activity,
        }
    }
}

/// Display projection of a milestone, with the rollups the page shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneRef {
    pub id: i64,
    pub project: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub target: Option<String>,
    pub total: i64,
    pub done: i64,
    pub last_activity: DateTime<Utc>,
    /// Issues moved to done per week, oldest week first (8 weeks) — the
    /// burndown sparkline's buckets, counted from the activity log.
    pub closes_8w: [i64; 8],
}

impl MilestoneRef {
    /// `done/total` as a percentage; an empty milestone reads as 0.
    pub fn percent(&self) -> u16 {
        if self.total == 0 {
            0
        } else {
            ((self.done * 100) / self.total) as u16
        }
    }

    /// The next status in the `C` cycle: open → completed → cancelled → open.
    pub fn next_status(&self) -> &'static str {
        match self.status.as_str() {
            "open" => "completed",
            "completed" => "cancelled",
            _ => "open",
        }
    }
}

/// Display projection of a project, with the rollups the project page shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRef {
    pub key: String,
    pub name: String,
    pub description: String,
    pub archived: bool,
    /// Active (board-visible) issues.
    pub total: i64,
    pub done: i64,
    pub milestones: i64,
    pub last_activity: DateTime<Utc>,
}

/// One reversible board mutation, recorded as its inverse. The stack is
/// session-local: undo is for the slip you just made, not time travel —
/// the activity log is the durable history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOp {
    /// The issue was moved; undoing moves it back to `from`.
    Move { key: String, from: String },
    /// A reorder swap is its own inverse.
    Reorder { key: String, other: String },
    /// The issue was archived; undoing unarchives it.
    Archive { key: String },
}

/// One relation edge of the card open in the detail popup, joined with the
/// other issue so the popup can say what it is and whether it's still open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationRef {
    /// `blocks` / `related_to` / `blocked_by` (read-side reverse).
    pub kind: String,
    pub key: String,
    pub title: String,
    pub status: String,
}

impl RelationRef {
    /// A blocker that still bites: not yet done.
    pub fn open_blocker(&self) -> bool {
        self.kind == "blocked_by" && self.status != "done"
    }
}

/// One row of the activity mailbox: an audit entry joined with its issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRef {
    pub issue_key: String,
    pub title: String,
    pub project: String,
    /// `status`, `edit`, `log`, `plan`, or `archive` — the CLI's audit kinds.
    pub kind: String,
    pub message: String,
    pub actor: Option<String>,
    pub ts: DateTime<Utc>,
}

impl ActivityRef {
    /// A status move that landed in done — what "issues closed" means. The
    /// message is `from → to` (optionally `: note`), so the last arrow's
    /// target is the landing status.
    pub fn closes(&self) -> bool {
        self.kind == "status"
            && self
                .message
                .rsplit('→')
                .next()
                .map(|t| t.trim_start().starts_with("done"))
                .unwrap_or(false)
    }
}

/// The activity page's view state. No sort: a mailbox is newest-first.
#[derive(Debug, Clone, Default)]
pub struct ActivityPageState {
    pub cursor: usize,
    pub query: String,
    pub filter: ActFilter,
}

/// Which events the mailbox lists. `Closed` is a *virtual* kind — status
/// entries whose move landed in done — because "what shipped recently" is
/// the question this page exists to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActFilter {
    #[default]
    All,
    Closed,
    Moves,
    Edits,
    Notes,
    Plan,
    Archive,
}

impl ActFilter {
    pub const ALL: &'static [Self] = &[
        Self::All,
        Self::Closed,
        Self::Moves,
        Self::Edits,
        Self::Notes,
        Self::Plan,
        Self::Archive,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Closed => "closed",
            Self::Moves => "moves",
            Self::Edits => "edits",
            Self::Notes => "notes",
            Self::Plan => "plan",
            Self::Archive => "archive",
        }
    }
    fn next(self) -> Self {
        let i = Self::ALL.iter().position(|f| *f == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
    fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|f| *f == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
    fn accepts(self, e: &ActivityRef) -> bool {
        match self {
            Self::All => true,
            Self::Closed => e.closes(),
            Self::Moves => e.kind == "status",
            Self::Edits => e.kind == "edit",
            Self::Notes => e.kind == "log",
            Self::Plan => e.kind == "plan",
            Self::Archive => e.kind == "archive",
        }
    }
}

/// The project page's view state — the same shape as [`MilestonePageState`].
#[derive(Debug, Clone, Default)]
pub struct ProjectPageState {
    pub cursor: usize,
    pub query: String,
    pub filter: ProjFilter,
    pub sort: ProjSort,
}

/// Which projects the page lists. Active is the working set; archived
/// projects are one Tab away rather than gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjFilter {
    #[default]
    Active,
    Archived,
    All,
}

impl ProjFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::All => "all",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Active => Self::Archived,
            Self::Archived => Self::All,
            Self::All => Self::Active,
        }
    }
    fn prev(self) -> Self {
        match self {
            Self::Active => Self::All,
            Self::Archived => Self::Active,
            Self::All => Self::Archived,
        }
    }
    fn accepts(self, archived: bool) -> bool {
        match self {
            Self::All => true,
            Self::Active => !archived,
            Self::Archived => archived,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjSort {
    /// Most recently worked on first.
    #[default]
    Activity,
    Name,
}

impl ProjSort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Activity => "activity",
            Self::Name => "name",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Activity => Self::Name,
            Self::Name => Self::Activity,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    Help,
    ConfirmQuit,
    ConfirmArchive(String), // `a` pressed; the key waiting to be archived
    /// `A` on the project page. Carries the page state so both answers land
    /// back on the page exactly as the user left it.
    ConfirmProjectArchive {
        key: String,
        /// The *new* archived state to apply on confirm.
        archived: bool,
        page: ProjectPageState,
    },
    AwaitingMove,   // Space pressed; waiting for column letter
    Detail(String), // focused card key
    MilestonePicker(PickerState),
    FuzzyFind(FuzzyState),
    MilestonePage(MilestonePageState),
    ProjectPage(ProjectPageState),
    ActivityPage(ActivityPageState),
}

#[derive(Debug, Clone)]
pub struct App {
    pub cards: Vec<Card>,
    pub milestones: Vec<MilestoneRef>, // for scope.project, name order
    pub projects: Vec<ProjectRef>,     // every project, activity order
    pub activity: Vec<ActivityRef>,    // audit entries, newest first
    /// Relations of the card open in `Mode::Detail`, loaded by the runtime
    /// when the popup opens; stale outside that mode and never rendered then.
    pub detail_relations: Vec<RelationRef>,
    pub focus: Focus,
    pub mode: Mode,
    pub scope: Scope,
    pub status_msg: Option<String>,
    pub last_card_idx_per_column: HashMap<ColumnId, usize>,
    pub pending_g: bool,
    pub boot_at: Instant,
    /// Everything at or before this instant has been read in the mailbox.
    pub last_seen: Option<DateTime<Utc>>,
    /// Where `last_seen` persists between sessions; `None` skips persistence
    /// (headless tests, hosts without a state dir).
    pub seen_path: Option<std::path::PathBuf>,
    /// `$CLIBAN_ACTOR` at startup — own events don't count as unread mail.
    pub self_actor: Option<String>,
    /// Reversible mutations this session, newest last (`u` pops).
    pub undo_stack: Vec<UndoOp>,
}

/// Undo remembers this many mutations before forgetting the oldest.
const UNDO_DEPTH: usize = 32;

impl App {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            milestones: Vec::new(),
            projects: Vec::new(),
            activity: Vec::new(),
            detail_relations: Vec::new(),
            focus: Focus::default(),
            mode: Mode::Normal,
            scope: Scope::default(),
            status_msg: None,
            last_card_idx_per_column: HashMap::new(),
            pending_g: false,
            boot_at: Instant::now(),
            last_seen: None,
            seen_path: None,
            self_actor: cliban_core::audit::actor(),
            undo_stack: Vec::new(),
        }
    }

    fn push_undo(&mut self, op: UndoOp) {
        self.undo_stack.push(op);
        if self.undo_stack.len() > UNDO_DEPTH {
            self.undo_stack.remove(0);
        }
    }

    /// Unread mail: activity newer than `last_seen`, excluding your own
    /// events — what you did yourself is not news to you.
    pub fn unseen_count(&self) -> usize {
        self.activity
            .iter()
            .filter(|e| match self.last_seen {
                Some(seen) => e.ts > seen,
                None => true,
            })
            .filter(|e| self.self_actor.is_none() || e.actor != self.self_actor)
            .count()
    }

    /// Opening (or leaving) the mailbox reads everything up to now.
    pub fn mark_seen(&mut self) {
        let now = Utc::now();
        self.last_seen = Some(now);
        if let Some(p) = &self.seen_path {
            crate::seen::store(p, now);
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
        let mut v: Vec<&Card> = self
            .cards
            .iter()
            .filter(|c| ColumnId::from_status(&c.status) == Some(col))
            .filter(|c| self.matches_scope(c))
            .collect();
        v.sort_by(|a, b| {
            a.position
                .partial_cmp(&b.position)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v
    }

    pub fn visible_columns(&self) -> Vec<ColumnId> {
        ColumnId::ALL.to_vec()
    }

    pub fn focused_card(&self) -> Option<&Card> {
        self.column_cards(self.focus.column)
            .into_iter()
            .nth(self.focus.card_idx)
    }

    pub fn blocked_count(&self) -> usize {
        self.column_cards(ColumnId::Blocked).len()
    }

    pub fn scoped_card_count(&self) -> usize {
        self.cards.iter().filter(|c| self.matches_scope(c)).count()
    }

    pub fn remember_cursor(&mut self) {
        self.last_card_idx_per_column
            .insert(self.focus.column, self.focus.card_idx);
    }
    pub fn restore_cursor_for(&self, col: ColumnId) -> usize {
        let len = self.column_cards(col).len();
        if len == 0 {
            return 0;
        }
        self.last_card_idx_per_column
            .get(&col)
            .copied()
            .unwrap_or(0)
            .min(len - 1)
    }
    pub fn auto_focus_if_empty(&mut self) {
        if self.focused_card().is_some() {
            return;
        }
        for col in self.visible_columns() {
            if !self.column_cards(col).is_empty() {
                self.focus.column = col;
                self.focus.card_idx = self.restore_cursor_for(col);
                return;
            }
        }
        self.focus.card_idx = 0;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Half-page step on the board: cards are four rows tall, so this is about
/// one screenful of a column.
const PAGE_STEP: usize = 5;
/// Half-page step on the milestone page, whose rows are single lines.
const MS_PAGE_STEP: usize = 10;

pub fn update(app: &mut App, action: Action) -> Option<Command> {
    // A status message answers the *previous* action; the next keypress
    // dismisses it. Arms that refuse below re-set it after this clear, and
    // the runtime posts its own messages after update returns, so anything
    // the user still sees is always about the last thing they did.
    app.status_msg = None;
    match action {
        Action::FocusMove(d) => {
            move_focus(app, d);
            None
        }
        Action::FocusColumn(i) => {
            // Direct jump lands even on an empty column (unlike h/l, which
            // skip them) — `n` there creates into that status.
            let columns = app.visible_columns();
            if let Some(&col) = columns.get(i) {
                app.remember_cursor();
                app.focus.column = col;
                app.focus.card_idx = app.restore_cursor_for(col);
            }
            None
        }
        Action::FocusCard(col, idx) => {
            app.remember_cursor();
            app.focus.column = col;
            app.focus.card_idx = idx.min(app.column_cards(col).len().saturating_sub(1));
            None
        }
        Action::PageMove(d) => {
            let last = app.column_cards(app.focus.column).len().saturating_sub(1);
            app.focus.card_idx = match d {
                Direction::Down => (app.focus.card_idx + PAGE_STEP).min(last),
                _ => app.focus.card_idx.saturating_sub(PAGE_STEP),
            };
            None
        }
        Action::JumpToTop => {
            app.focus.card_idx = 0;
            None
        }
        Action::JumpToBottom => {
            app.focus.card_idx = app.column_cards(app.focus.column).len().saturating_sub(1);
            None
        }
        Action::ToggleHelp => {
            app.mode = match app.mode {
                Mode::Help => Mode::Normal,
                _ => Mode::Help,
            };
            None
        }
        Action::QuitRequest => {
            app.mode = Mode::ConfirmQuit;
            None
        }
        Action::Quit => None,
        Action::Cancel => {
            // Declining the project-archive dialog returns to the page it
            // came from; every other cancel unwinds to the board.
            app.mode = match &app.mode {
                Mode::ConfirmProjectArchive { page, .. } => Mode::ProjectPage(page.clone()),
                _ => Mode::Normal,
            };
            None
        }
        Action::Refresh => Some(Command::Reload),
        Action::OpenDetail => {
            if let Some(c) = app.focused_card() {
                app.mode = Mode::Detail(c.key.clone());
            }
            None
        }
        Action::JumpToBlocker => {
            // From the detail popup: land the board cursor on the first
            // still-open blocker; explain when it isn't reachable.
            let target = app
                .detail_relations
                .iter()
                .find(|r| r.open_blocker())
                .map(|r| r.key.clone())?;
            match locate_focus_for_key(app, &target) {
                Some(focus) => {
                    app.focus = focus;
                    app.mode = Mode::Normal;
                }
                None => {
                    app.status_msg = Some(format!(
                        "{target} isn't on the board (archived or out of scope)"
                    ));
                }
            }
            None
        }
        Action::BeginMove => {
            app.mode = Mode::AwaitingMove;
            None
        }
        Action::MoveTo(status) => {
            app.mode = Mode::Normal;
            let card = app.focused_card()?;
            let (key, from) = (card.key.clone(), card.status.clone());
            app.push_undo(UndoOp::Move {
                key: key.clone(),
                from,
            });
            Some(Command::MoveIssue { key, status })
        }
        Action::MoveIssueDir(d) => {
            let key = app.focused_card()?.key.clone();
            match d {
                Direction::Left | Direction::Right => {
                    let cur = ColumnId::ALL.iter().position(|c| *c == app.focus.column)?;
                    let target_idx = match d {
                        Direction::Left => cur.checked_sub(1)?,
                        _ => {
                            let n = cur + 1;
                            if n >= ColumnId::ALL.len() {
                                return None;
                            }
                            n
                        }
                    };
                    let target = ColumnId::ALL[target_idx];
                    let status = target.status().to_string();
                    let from = app.focus.column.status().to_string();
                    // core's move_issue appends to the end of the target column, so the
                    // cursor follows the card to its landing slot there.
                    let landing = app.column_cards(target).len();
                    app.focus.column = target;
                    app.focus.card_idx = landing;
                    app.push_undo(UndoOp::Move {
                        key: key.clone(),
                        from,
                    });
                    Some(Command::MoveIssue { key, status })
                }
                Direction::Down | Direction::Up => {
                    let cards = app.column_cards(app.focus.column);
                    let idx = app.focus.card_idx;
                    let other_idx = match d {
                        Direction::Down => {
                            let n = idx + 1;
                            if n >= cards.len() {
                                return None;
                            }
                            n
                        }
                        _ => idx.checked_sub(1)?,
                    };
                    let other = cards[other_idx].key.clone();
                    app.focus.card_idx = other_idx; // cursor follows the reordered card
                    app.push_undo(UndoOp::Reorder {
                        key: key.clone(),
                        other: other.clone(),
                    });
                    Some(Command::Reorder { key, other })
                }
            }
        }
        Action::ArchiveRequest => {
            // Archive is the one board action with no undo from the TUI, so
            // it gets a confirm — everything else stays one keypress.
            let key = app.focused_card()?.key.clone();
            app.mode = Mode::ConfirmArchive(key);
            None
        }
        Action::Archive => {
            let key = match &app.mode {
                Mode::ConfirmArchive(k) => k.clone(),
                _ => app.focused_card()?.key.clone(),
            };
            app.mode = Mode::Normal;
            app.push_undo(UndoOp::Archive { key: key.clone() });
            Some(Command::Archive { key })
        }
        Action::Undo => {
            let Some(op) = app.undo_stack.pop() else {
                app.status_msg = Some("nothing to undo".into());
                return None;
            };
            match op {
                UndoOp::Move { key, from } => {
                    app.status_msg = Some(format!("undid: {key} back to {from}"));
                    Some(Command::MoveIssue { key, status: from })
                }
                UndoOp::Reorder { key, other } => {
                    app.status_msg = Some(format!("undid: reorder of {key}"));
                    Some(Command::Reorder { key, other })
                }
                UndoOp::Archive { key } => {
                    app.status_msg = Some(format!("undid: unarchived {key}"));
                    Some(Command::Unarchive { key })
                }
            }
        }
        Action::EditCard => {
            let key = app.focused_card()?.key.clone();
            Some(Command::EditIssue { key })
        }
        Action::EditScope => match (&app.scope.project, &app.scope.milestone) {
            (Some(p), Some(m)) => Some(Command::EditMilestone {
                project: p.clone(),
                name: m.clone(),
            }),
            (Some(p), None) => Some(Command::EditProject { key: p.clone() }),
            (None, _) => {
                app.status_msg = Some("scope a project first (p) to edit it".into());
                None
            }
        },
        Action::NewIssue => {
            let status = app.focus.column.status().to_string();
            Some(Command::NewIssue { status })
        }
        Action::NewMilestone => Some(Command::NewMilestone {
            project: app.scope.project.clone(),
        }),
        Action::TagMilestone => {
            let card = app.focused_card()?.clone();
            // Only this card's own project's milestones are taggable: with no
            // project scoped, `app.milestones` spans every project.
            let names: Vec<&MilestoneRef> = app
                .milestones
                .iter()
                .filter(|m| m.project == card.project)
                .collect();
            if names.is_empty() {
                app.status_msg = Some("no milestones to tag (N to create one)".into());
                return None;
            }
            let cur = card
                .milestone_id
                .and_then(|id| names.iter().position(|m| m.id == id))
                .map(|i| i + 1)
                .unwrap_or(0);
            let next = (cur + 1) % (names.len() + 1);
            let milestone = if next == 0 {
                None
            } else {
                Some(names[next - 1].name.clone())
            };
            Some(Command::TagMilestone {
                key: card.key,
                milestone,
            })
        }
        _ => update_overlays(app, action),
    }
}

fn update_overlays(app: &mut App, action: Action) -> Option<Command> {
    match action {
        Action::OpenProjectPage => {
            app.mode = Mode::ProjectPage(ProjectPageState::default());
            None
        }
        Action::OpenActivityPage => {
            app.mode = Mode::ActivityPage(ActivityPageState::default());
            None
        }
        Action::OpenMilestonePicker => {
            if app.scope.project.is_none() {
                app.status_msg = Some("scope a project first with p".into());
                return None;
            }
            app.mode = Mode::MilestonePicker(PickerState {
                query: String::new(),
                items: vec![],
                cursor: 0,
            });
            None
        }
        Action::OpenMilestonePage => {
            app.mode = Mode::MilestonePage(MilestonePageState::default());
            None
        }
        Action::CycleMilestoneFilter => {
            let Some(project) = app.scope.project.clone() else {
                app.status_msg = Some("scope a project first with p".into());
                return None;
            };
            // Name order, so repeated `M` walks a stable cycle regardless of
            // how `app.milestones` happens to be sorted.
            let mut scoped: Vec<&MilestoneRef> = app
                .milestones
                .iter()
                .filter(|m| m.project == project)
                .collect();
            scoped.sort_by(|a, b| a.name.cmp(&b.name));
            if scoped.is_empty() {
                return None;
            }
            let names: Vec<&str> = scoped.iter().map(|m| m.name.as_str()).collect();
            let next = match &app.scope.milestone {
                None => Some(names[0].to_string()),
                Some(cur) => match names.iter().position(|n| *n == cur.as_str()) {
                    Some(i) if i + 1 < names.len() => Some(names[i + 1].to_string()),
                    _ => None,
                },
            };
            app.scope.milestone = next;
            app.auto_focus_if_empty();
            Some(Command::SetScope)
        }
        Action::SetScope(s) => {
            app.scope = s;
            app.auto_focus_if_empty();
            Some(Command::SetScope)
        }
        Action::PickerInput(c) => {
            with_picker(app, |p| {
                p.query.push(c);
                p.cursor = 0;
            });
            None
        }
        Action::PickerBackspace => {
            with_picker(app, |p| {
                p.query.pop();
                p.cursor = 0;
            });
            None
        }
        Action::PickerUp => {
            with_picker(app, |p| p.cursor = p.cursor.saturating_sub(1));
            None
        }
        Action::PickerDown => {
            with_picker(app, |p| {
                let n = filtered_picker(p).len();
                if p.cursor + 1 < n {
                    p.cursor += 1;
                }
            });
            None
        }
        Action::PickerConfirm => picker_confirm(app),
        _ => update_fuzzy_overlay(app, action),
    }
}

fn with_picker(app: &mut App, f: impl FnOnce(&mut PickerState)) {
    if let Mode::MilestonePicker(p) = &mut app.mode {
        f(p)
    }
}

/// Substring filter over picker labels; returns indices into `items`.
pub fn filtered_picker(p: &PickerState) -> Vec<usize> {
    if p.query.is_empty() {
        return (0..p.items.len()).collect();
    }
    let q = p.query.to_lowercase();
    p.items
        .iter()
        .enumerate()
        .filter(|(_, c)| c.label.to_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
}

/// The milestone page's visible rows, as indices into `app.milestones`:
/// status bucket, then a case-insensitive substring match on name (or project
/// key), then the chosen ordering. `app.milestones` already arrives sorted by
/// activity, so [`PageSort::Activity`] just preserves that order.
pub fn page_rows(app: &App, state: &MilestonePageState) -> Vec<usize> {
    let q = state.query.to_lowercase();
    let mut idx: Vec<usize> = app
        .milestones
        .iter()
        .enumerate()
        .filter(|(_, m)| state.filter.accepts(&m.status))
        .filter(|(_, m)| {
            q.is_empty()
                || m.name.to_lowercase().contains(&q)
                || m.project.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect();
    match state.sort {
        PageSort::Activity => {}
        PageSort::Name => idx.sort_by(|&a, &b| {
            let (a, b) = (&app.milestones[a], &app.milestones[b]);
            (&a.project, &a.name).cmp(&(&b.project, &b.name))
        }),
        // Undated milestones sort last; `YYYY-MM-DD` compares lexicographically.
        PageSort::Target => idx.sort_by(|&a, &b| {
            let (a, b) = (&app.milestones[a], &app.milestones[b]);
            (a.target.is_none(), &a.target, &a.name).cmp(&(b.target.is_none(), &b.target, &b.name))
        }),
    }
    idx
}

/// The milestone the page cursor is on, if any.
pub fn page_focused<'a>(app: &'a App, state: &MilestonePageState) -> Option<&'a MilestoneRef> {
    let rows = page_rows(app, state);
    app.milestones.get(*rows.get(state.cursor)?)
}

fn picker_confirm(app: &mut App) -> Option<Command> {
    let chip = match &app.mode {
        Mode::MilestonePicker(p) => {
            let i = *filtered_picker(p).get(p.cursor)?;
            p.items[i].clone()
        }
        _ => return None,
    };
    app.mode = Mode::Normal;
    app.scope.milestone = Some(chip.value);
    app.auto_focus_if_empty();
    Some(Command::SetScope)
}

fn update_fuzzy_overlay(app: &mut App, action: Action) -> Option<Command> {
    match action {
        Action::OpenFuzzyFind => {
            let results = fuzzy_search(app, "");
            app.mode = Mode::FuzzyFind(FuzzyState {
                query: String::new(),
                results,
                cursor: 0,
            });
            None
        }
        Action::FuzzyInput(c) => {
            let q = match &app.mode {
                Mode::FuzzyFind(f) => {
                    let mut q = f.query.clone();
                    q.push(c);
                    q
                }
                _ => return None,
            };
            let r = fuzzy_search(app, &q);
            if let Mode::FuzzyFind(f) = &mut app.mode {
                f.query = q;
                f.results = r;
                f.cursor = 0;
            }
            None
        }
        Action::FuzzyBackspace => {
            let q = match &app.mode {
                Mode::FuzzyFind(f) => {
                    let mut q = f.query.clone();
                    q.pop();
                    q
                }
                _ => return None,
            };
            let r = fuzzy_search(app, &q);
            if let Mode::FuzzyFind(f) = &mut app.mode {
                f.query = q;
                f.results = r;
                f.cursor = 0;
            }
            None
        }
        Action::FuzzyUp => {
            if let Mode::FuzzyFind(f) = &mut app.mode {
                f.cursor = f.cursor.saturating_sub(1);
            }
            None
        }
        Action::FuzzyDown => {
            if let Mode::FuzzyFind(f) = &mut app.mode {
                let m = f.results.len().saturating_sub(1);
                if f.cursor < m {
                    f.cursor += 1;
                }
            }
            None
        }
        Action::FuzzyConfirm => {
            let target = match &app.mode {
                Mode::FuzzyFind(f) => f.results.get(f.cursor).cloned()?,
                _ => return None,
            };
            if let Some(focus) = locate_focus_for_key(app, &target) {
                app.focus = focus;
            }
            app.mode = Mode::Normal;
            None
        }
        _ => update_milestone_page(app, action),
    }
}

/// The milestone page. Every arm re-derives its target from `app.milestones`
/// through [`page_rows`], so the page always acts on what is on screen right
/// now — including after a reload reorders the list.
fn update_milestone_page(app: &mut App, action: Action) -> Option<Command> {
    // Read-only arms first: they need `&App` while the state lives in `app.mode`.
    let state = match &app.mode {
        Mode::MilestonePage(s) => s.clone(),
        // Not this page's mode — hand the action to the project page, the
        // last stop in the dispatch chain.
        _ => return update_project_page(app, action),
    };
    match action {
        Action::MsPageInput(c) => {
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.query.push(c);
                s.cursor = 0;
            }
            None
        }
        Action::MsPageBackspace => {
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.query.pop();
                s.cursor = 0;
            }
            None
        }
        Action::PageCursor(i) => {
            let last = page_rows(app, &state).len().saturating_sub(1);
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.cursor = i.min(last);
            }
            None
        }
        Action::MsPageUp => {
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.cursor = s.cursor.saturating_sub(1);
            }
            None
        }
        Action::MsPageDown => {
            let last = page_rows(app, &state).len().saturating_sub(1);
            if let Mode::MilestonePage(s) = &mut app.mode {
                if s.cursor < last {
                    s.cursor += 1;
                }
            }
            None
        }
        Action::MsPagePage(d) => {
            let last = page_rows(app, &state).len().saturating_sub(1);
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.cursor = match d {
                    Direction::Down => (s.cursor + MS_PAGE_STEP).min(last),
                    _ => s.cursor.saturating_sub(MS_PAGE_STEP),
                };
            }
            None
        }
        Action::MsPageTop => {
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.cursor = 0;
            }
            None
        }
        Action::MsPageBottom => {
            let last = page_rows(app, &state).len().saturating_sub(1);
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.cursor = last;
            }
            None
        }
        Action::MsPageCycleFilter => {
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.filter = s.filter.next();
                s.cursor = 0;
            }
            None
        }
        Action::MsPageCycleFilterBack => {
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.filter = s.filter.prev();
                s.cursor = 0;
            }
            None
        }
        Action::MsPageCycleSort => {
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.sort = s.sort.next();
                s.cursor = 0;
            }
            None
        }
        Action::MsPageEdit => {
            let m = page_focused(app, &state)?;
            Some(Command::EditMilestone {
                project: m.project.clone(),
                name: m.name.clone(),
            })
        }
        Action::MsPageNew => Some(Command::NewMilestone {
            // Create into whatever the cursor is sitting in, so the page works
            // unscoped; fall back to the board's scope on an empty list.
            project: page_focused(app, &state)
                .map(|m| m.project.clone())
                .or_else(|| app.scope.project.clone()),
        }),
        Action::MsPageCycleStatus => {
            let m = page_focused(app, &state)?;
            Some(Command::SetMilestoneStatus {
                project: m.project.clone(),
                name: m.name.clone(),
                status: m.next_status().to_string(),
            })
        }
        Action::MsPageSelect => {
            let m = page_focused(app, &state)?;
            let (project, name) = (m.project.clone(), m.name.clone());
            // Selecting a milestone from another project re-scopes the board
            // to that project too, keeping the Scope invariant honest.
            app.scope.set_project(Some(project));
            app.scope.milestone = Some(name);
            app.mode = Mode::Normal;
            app.auto_focus_if_empty();
            Some(Command::SetScope)
        }
        _ => update_project_page(app, action),
    }
}

/// The project page's visible rows, as indices into `app.projects`: archive
/// bucket, then a case-insensitive substring match on key or name, then the
/// chosen ordering. `app.projects` already arrives sorted by activity.
pub fn project_rows(app: &App, state: &ProjectPageState) -> Vec<usize> {
    let q = state.query.to_lowercase();
    let mut idx: Vec<usize> = app
        .projects
        .iter()
        .enumerate()
        .filter(|(_, p)| state.filter.accepts(p.archived))
        .filter(|(_, p)| {
            q.is_empty() || p.key.to_lowercase().contains(&q) || p.name.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect();
    if state.sort == ProjSort::Name {
        idx.sort_by(|&a, &b| app.projects[a].key.cmp(&app.projects[b].key));
    }
    idx
}

/// The project the page cursor is on, if any.
pub fn project_focused<'a>(app: &'a App, state: &ProjectPageState) -> Option<&'a ProjectRef> {
    let rows = project_rows(app, state);
    app.projects.get(*rows.get(state.cursor)?)
}

/// The project page. Same contract as the milestone page: every arm
/// re-derives its target from `app.projects` through [`project_rows`].
fn update_project_page(app: &mut App, action: Action) -> Option<Command> {
    // The archive dialog carries its own target and page state, so it is
    // handled before the page-state guard below.
    if let Action::ProjPageArchive = action {
        let Mode::ConfirmProjectArchive {
            key,
            archived,
            page,
        } = app.mode.clone()
        else {
            return None;
        };
        // Archiving the project the board is scoped to would leave the board
        // pointed at something the page's active bucket no longer offers —
        // unscope it.
        if archived && app.scope.project.as_deref() == Some(key.as_str()) {
            app.scope.set_project(None);
        }
        app.mode = Mode::ProjectPage(page);
        return Some(Command::SetProjectArchived { key, archived });
    }

    let state = match &app.mode {
        Mode::ProjectPage(s) => s.clone(),
        // Not this page's mode — hand the action to the activity page, the
        // last stop in the dispatch chain.
        _ => return update_activity_page(app, action),
    };
    match action {
        Action::ProjPageInput(c) => {
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.query.push(c);
                s.cursor = 0;
            }
            None
        }
        Action::ProjPageBackspace => {
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.query.pop();
                s.cursor = 0;
            }
            None
        }
        Action::PageCursor(i) => {
            let last = project_rows(app, &state).len().saturating_sub(1);
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.cursor = i.min(last);
            }
            None
        }
        Action::ProjPageUp => {
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.cursor = s.cursor.saturating_sub(1);
            }
            None
        }
        Action::ProjPageDown => {
            let last = project_rows(app, &state).len().saturating_sub(1);
            if let Mode::ProjectPage(s) = &mut app.mode {
                if s.cursor < last {
                    s.cursor += 1;
                }
            }
            None
        }
        Action::ProjPagePage(d) => {
            let last = project_rows(app, &state).len().saturating_sub(1);
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.cursor = match d {
                    Direction::Down => (s.cursor + MS_PAGE_STEP).min(last),
                    _ => s.cursor.saturating_sub(MS_PAGE_STEP),
                };
            }
            None
        }
        Action::ProjPageTop => {
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.cursor = 0;
            }
            None
        }
        Action::ProjPageBottom => {
            let last = project_rows(app, &state).len().saturating_sub(1);
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.cursor = last;
            }
            None
        }
        Action::ProjPageCycleFilter => {
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.filter = s.filter.next();
                s.cursor = 0;
            }
            None
        }
        Action::ProjPageCycleFilterBack => {
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.filter = s.filter.prev();
                s.cursor = 0;
            }
            None
        }
        Action::ProjPageCycleSort => {
            if let Mode::ProjectPage(s) = &mut app.mode {
                s.sort = s.sort.next();
                s.cursor = 0;
            }
            None
        }
        Action::ProjPageEdit => {
            let p = project_focused(app, &state)?;
            Some(Command::EditProject { key: p.key.clone() })
        }
        Action::ProjPageNew => Some(Command::NewProject),
        Action::ProjPageArchiveRequest => {
            let p = project_focused(app, &state)?;
            app.mode = Mode::ConfirmProjectArchive {
                key: p.key.clone(),
                archived: !p.archived,
                page: state.clone(),
            };
            None
        }
        Action::ProjPageSelect => {
            let p = project_focused(app, &state)?;
            if p.archived {
                app.status_msg = Some("unarchive it (A) before scoping the board to it".into());
                return None;
            }
            let key = p.key.clone();
            app.scope.set_project(Some(key));
            app.mode = Mode::Normal;
            app.auto_focus_if_empty();
            Some(Command::SetScope)
        }
        _ => update_activity_page(app, action),
    }
}

/// The activity mailbox's visible rows, as indices into `app.activity`:
/// event-kind bucket, then a case-insensitive substring match on issue key,
/// title, message, or actor. Entries already arrive newest first.
pub fn activity_rows(app: &App, state: &ActivityPageState) -> Vec<usize> {
    let q = state.query.to_lowercase();
    app.activity
        .iter()
        .enumerate()
        .filter(|(_, e)| state.filter.accepts(e))
        .filter(|(_, e)| {
            q.is_empty()
                || e.issue_key.to_lowercase().contains(&q)
                || e.title.to_lowercase().contains(&q)
                || e.message.to_lowercase().contains(&q)
                || e.actor.as_deref().unwrap_or("").to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

/// The entry the mailbox cursor is on, if any.
pub fn activity_focused<'a>(app: &'a App, state: &ActivityPageState) -> Option<&'a ActivityRef> {
    let rows = activity_rows(app, state);
    app.activity.get(*rows.get(state.cursor)?)
}

/// The activity page — the last stop in the page dispatch chain.
fn update_activity_page(app: &mut App, action: Action) -> Option<Command> {
    let state = match &app.mode {
        Mode::ActivityPage(s) => s.clone(),
        _ => return None,
    };
    match action {
        Action::ActPageInput(c) => {
            if let Mode::ActivityPage(s) = &mut app.mode {
                s.query.push(c);
                s.cursor = 0;
            }
            None
        }
        Action::ActPageBackspace => {
            if let Mode::ActivityPage(s) = &mut app.mode {
                s.query.pop();
                s.cursor = 0;
            }
            None
        }
        Action::PageCursor(i) => {
            let last = activity_rows(app, &state).len().saturating_sub(1);
            if let Mode::ActivityPage(s) = &mut app.mode {
                s.cursor = i.min(last);
            }
            None
        }
        Action::ActPageUp => {
            if let Mode::ActivityPage(s) = &mut app.mode {
                s.cursor = s.cursor.saturating_sub(1);
            }
            None
        }
        Action::ActPageDown => {
            let last = activity_rows(app, &state).len().saturating_sub(1);
            if let Mode::ActivityPage(s) = &mut app.mode {
                if s.cursor < last {
                    s.cursor += 1;
                }
            }
            None
        }
        Action::ActPagePage(d) => {
            let last = activity_rows(app, &state).len().saturating_sub(1);
            if let Mode::ActivityPage(s) = &mut app.mode {
                s.cursor = match d {
                    Direction::Down => (s.cursor + MS_PAGE_STEP).min(last),
                    _ => s.cursor.saturating_sub(MS_PAGE_STEP),
                };
            }
            None
        }
        Action::ActPageTop => {
            if let Mode::ActivityPage(s) = &mut app.mode {
                s.cursor = 0;
            }
            None
        }
        Action::ActPageBottom => {
            let last = activity_rows(app, &state).len().saturating_sub(1);
            if let Mode::ActivityPage(s) = &mut app.mode {
                s.cursor = last;
            }
            None
        }
        Action::ActPageCycleFilter => {
            if let Mode::ActivityPage(s) = &mut app.mode {
                s.filter = s.filter.next();
                s.cursor = 0;
            }
            None
        }
        Action::ActPageCycleFilterBack => {
            if let Mode::ActivityPage(s) = &mut app.mode {
                s.filter = s.filter.prev();
                s.cursor = 0;
            }
            None
        }
        Action::ActPageSelect => {
            let e = activity_focused(app, &state)?;
            let key = e.issue_key.clone();
            // Jump the board cursor to the issue, if it's visible under the
            // current scope. Archived or scoped-out issues stay browsable
            // here but explain why Enter won't land.
            match locate_focus_for_key(app, &key) {
                Some(focus) => {
                    app.focus = focus;
                    app.mode = Mode::Normal;
                    None
                }
                None => {
                    app.status_msg = Some(format!(
                        "{key} isn't on the board (archived or out of scope)"
                    ));
                    None
                }
            }
        }
        _ => None,
    }
}

pub fn fuzzy_search(app: &App, query: &str) -> Vec<String> {
    let needle = query.to_lowercase();
    let mut out = Vec::new();
    for col in app.visible_columns() {
        for c in app.column_cards(col) {
            let hay = format!("{} {}", c.key, c.title).to_lowercase();
            if needle.is_empty() || hay.contains(&needle) {
                out.push(c.key.clone());
            }
        }
    }
    out
}

fn locate_focus_for_key(app: &App, key: &str) -> Option<Focus> {
    for col in app.visible_columns() {
        for (idx, c) in app.column_cards(col).iter().enumerate() {
            if c.key == key {
                return Some(Focus {
                    column: col,
                    card_idx: idx,
                });
            }
        }
    }
    None
}

fn move_focus(app: &mut App, dir: Direction) {
    let columns = app.visible_columns();
    let cur = columns
        .iter()
        .position(|c| *c == app.focus.column)
        .unwrap_or(0);
    match dir {
        Direction::Left => {
            if let Some(t) = (0..cur)
                .rev()
                .find(|&i| !app.column_cards(columns[i]).is_empty())
            {
                app.remember_cursor();
                app.focus.column = columns[t];
                app.focus.card_idx = app.restore_cursor_for(columns[t]);
            }
        }
        Direction::Right => {
            if let Some(t) =
                (cur + 1..columns.len()).find(|&i| !app.column_cards(columns[i]).is_empty())
            {
                app.remember_cursor();
                app.focus.column = columns[t];
                app.focus.card_idx = app.restore_cursor_for(columns[t]);
            }
        }
        Direction::Up => {
            if app.focus.card_idx > 0 {
                app.focus.card_idx -= 1;
            }
        }
        Direction::Down => {
            let n = app.column_cards(app.focus.column).len();
            if app.focus.card_idx + 1 < n {
                app.focus.card_idx += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `CLI` milestone with no issues and no target.
    fn milestone(name: &str, status: &str) -> MilestoneRef {
        MilestoneRef {
            id: name.len() as i64,
            project: "CLI".into(),
            name: name.into(),
            description: String::new(),
            status: status.into(),
            target: None,
            total: 0,
            done: 0,
            last_activity: DateTime::UNIX_EPOCH,
            closes_8w: [0; 8],
        }
    }

    /// The same, but in a different project — for the cross-project paths.
    fn other_project(name: &str) -> MilestoneRef {
        MilestoneRef {
            project: "LM".into(),
            ..milestone(name, "open")
        }
    }

    fn card(key: &str, status: &str, pos: f64) -> Card {
        let project = key.split('-').next().unwrap().to_string();
        Card {
            id: 0,
            key: key.into(),
            project,
            title: format!("T {key}"),
            status: status.into(),
            priority: "medium".into(),
            position: pos,
            milestone_id: None,
            milestone: None,
            labels: Vec::new(),
        }
    }

    #[test]
    fn column_cards_sorted_and_scope_filtered() {
        let mut app = App::new();
        app.cards = vec![
            card("CLI-2", "backlog", 2000.0),
            card("CLI-1", "backlog", 1000.0),
            card("LM-1", "backlog", 500.0),
        ];
        let backlog = app.column_cards(ColumnId::Backlog);
        assert_eq!(
            backlog.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(),
            vec!["LM-1", "CLI-1", "CLI-2"]
        );
        app.scope.set_project(Some("CLI".into()));
        let scoped = app.column_cards(ColumnId::Backlog);
        assert_eq!(
            scoped.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(),
            vec!["CLI-1", "CLI-2"]
        );
    }

    #[test]
    fn column_status_round_trips() {
        for c in ColumnId::ALL {
            assert_eq!(ColumnId::from_status(c.status()), Some(*c));
        }
        assert_eq!(ColumnId::from_status("nope"), None);
    }

    #[test]
    fn scope_clearing_project_clears_milestone() {
        let mut s = Scope {
            project: Some("CLI".into()),
            milestone: Some("M1".into()),
        };
        s.set_project(None);
        assert_eq!(s.milestone, None);
    }

    use crate::actions::{Action, Command, Direction};

    #[test]
    fn space_then_letter_moves_focused_card() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1", "backlog", 1000.0)];
        app.auto_focus_if_empty();
        assert!(update(&mut app, Action::BeginMove).is_none());
        assert!(matches!(app.mode, Mode::AwaitingMove));
        match update(&mut app, Action::MoveTo("in-progress".into())).unwrap() {
            Command::MoveIssue { key, status } => {
                assert_eq!(key, "CLI-1");
                assert_eq!(status, "in-progress");
            }
            _ => panic!(),
        }
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn capital_l_moves_issue_to_next_column_and_cursor_follows() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1", "backlog", 1000.0)];
        app.auto_focus_if_empty();
        match update(&mut app, Action::MoveIssueDir(Direction::Right)).unwrap() {
            Command::MoveIssue { key, status } => {
                assert_eq!(key, "CLI-1");
                assert_eq!(status, "in-progress");
            }
            _ => panic!(),
        }
        assert_eq!(app.focus.column, ColumnId::InProgress);
        assert_eq!(app.focus.card_idx, 0); // landing slot in the (was empty) target column
    }

    #[test]
    fn capital_h_at_leftmost_column_is_noop() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1", "backlog", 1000.0)];
        app.auto_focus_if_empty();
        assert!(update(&mut app, Action::MoveIssueDir(Direction::Left)).is_none());
        assert_eq!(app.focus.column, ColumnId::Backlog);
    }

    #[test]
    fn capital_j_reorders_within_column_and_cursor_follows() {
        let mut app = App::new();
        app.cards = vec![
            card("CLI-1", "backlog", 1000.0),
            card("CLI-2", "backlog", 2000.0),
        ];
        app.scope.set_project(Some("CLI".into()));
        app.auto_focus_if_empty();
        assert_eq!(app.focus.card_idx, 0); // on CLI-1
        match update(&mut app, Action::MoveIssueDir(Direction::Down)).unwrap() {
            Command::Reorder { key, other } => {
                assert_eq!(key, "CLI-1");
                assert_eq!(other, "CLI-2");
            }
            _ => panic!(),
        }
        assert_eq!(app.focus.card_idx, 1); // cursor follows the card down
    }

    #[test]
    fn capital_j_at_bottom_of_column_is_noop() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1", "backlog", 1000.0)];
        app.scope.set_project(Some("CLI".into()));
        app.auto_focus_if_empty();
        assert!(update(&mut app, Action::MoveIssueDir(Direction::Down)).is_none());
    }

    #[test]
    fn tag_milestone_cycles_none_to_first() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1", "backlog", 1000.0)];
        app.milestones = vec![milestone("M1", "open")];
        app.auto_focus_if_empty();
        match update(&mut app, Action::TagMilestone).unwrap() {
            Command::TagMilestone { key, milestone } => {
                assert_eq!(key, "CLI-1");
                assert_eq!(milestone, Some("M1".to_string()));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn tag_milestone_ignores_other_projects_milestones() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1", "backlog", 1000.0)];
        // Unscoped, `app.milestones` spans projects; only CLI's are taggable.
        app.milestones = vec![other_project("LM-only")];
        app.auto_focus_if_empty();
        assert!(update(&mut app, Action::TagMilestone).is_none());
        assert!(app.status_msg.is_some());
    }

    #[test]
    fn fuzzy_search_matches_key_and_title() {
        let mut app = App::new();
        let mut c = card("CLI-1", "backlog", 1.0);
        c.title = "Build the board".into();
        app.cards = vec![c];
        assert_eq!(fuzzy_search(&app, "board"), vec!["CLI-1"]);
        assert_eq!(fuzzy_search(&app, "cli-1"), vec!["CLI-1"]);
        assert!(fuzzy_search(&app, "zzz").is_empty());
    }

    #[test]
    fn focus_move_right_skips_empty_columns() {
        let mut app = App::new();
        app.cards = vec![card("CLI-1", "blocked", 1.0)];
        app.focus = Focus {
            column: ColumnId::Backlog,
            card_idx: 0,
        };
        update(&mut app, Action::FocusMove(Direction::Right));
        assert_eq!(app.focus.column, ColumnId::Blocked);
    }

    #[test]
    fn cycle_milestone_filter_advances_then_wraps_to_all() {
        let mut app = App::new();
        app.scope.set_project(Some("CLI".into()));
        app.milestones = vec![milestone("M1", "open"), milestone("M2", "open")];
        update(&mut app, Action::CycleMilestoneFilter);
        assert_eq!(app.scope.milestone.as_deref(), Some("M1"));
        update(&mut app, Action::CycleMilestoneFilter);
        assert_eq!(app.scope.milestone.as_deref(), Some("M2"));
        update(&mut app, Action::CycleMilestoneFilter);
        assert_eq!(app.scope.milestone, None);
    }

    #[test]
    fn cycle_milestone_filter_only_walks_the_scoped_project() {
        let mut app = App::new();
        app.milestones = vec![milestone("M1", "open"), other_project("LM-only")];
        // Unscoped: nothing to cycle, and the user is told why.
        assert!(update(&mut app, Action::CycleMilestoneFilter).is_none());
        assert_eq!(app.scope.milestone, None);
        assert!(app.status_msg.is_some());

        app.scope.set_project(Some("CLI".into()));
        update(&mut app, Action::CycleMilestoneFilter);
        assert_eq!(app.scope.milestone.as_deref(), Some("M1"));
        update(&mut app, Action::CycleMilestoneFilter);
        assert_eq!(
            app.scope.milestone, None,
            "the other project's milestone is not in the cycle"
        );
    }

    fn page(app: &App) -> &MilestonePageState {
        match &app.mode {
            Mode::MilestonePage(s) => s,
            _ => panic!("expected the milestone page"),
        }
    }

    #[test]
    fn page_opens_on_the_open_bucket_in_activity_order() {
        let mut app = App::new();
        app.milestones = vec![
            milestone("open-one", "open"),
            milestone("done-one", "completed"),
            milestone("axed-one", "cancelled"),
        ];
        update(&mut app, Action::OpenMilestonePage);
        let s = page(&app);
        assert_eq!(s.filter, StatusFilter::Open);
        assert_eq!(s.sort, PageSort::Activity);
        assert_eq!(page_rows(&app, s), vec![0], "only the open milestone shows");
    }

    #[test]
    fn cycling_the_status_bucket_walks_open_completed_cancelled_all() {
        let mut app = App::new();
        app.milestones = vec![
            milestone("open-one", "open"),
            milestone("done-one", "completed"),
            milestone("axed-one", "cancelled"),
        ];
        update(&mut app, Action::OpenMilestonePage);
        for (expect_filter, expect_rows) in [
            (StatusFilter::Completed, vec![1]),
            (StatusFilter::Cancelled, vec![2]),
            (StatusFilter::All, vec![0, 1, 2]),
            (StatusFilter::Open, vec![0]),
        ] {
            update(&mut app, Action::MsPageCycleFilter);
            let s = page(&app);
            assert_eq!(s.filter, expect_filter);
            assert_eq!(page_rows(&app, s), expect_rows);
        }
    }

    #[test]
    fn sorting_reorders_by_name_then_target_then_back_to_activity() {
        let mut app = App::new();
        // Load order is activity order (what `data::load_milestones` returns).
        let mut zeta = milestone("zeta", "open");
        zeta.target = Some("2026-01-01".into());
        let mut alpha = milestone("alpha", "open");
        alpha.target = None;
        let mut mid = milestone("mid", "open");
        mid.target = Some("2026-06-01".into());
        app.milestones = vec![zeta, alpha, mid];
        update(&mut app, Action::OpenMilestonePage);

        assert_eq!(page_rows(&app, page(&app)), vec![0, 1, 2], "activity order");
        update(&mut app, Action::MsPageCycleSort);
        assert_eq!(page(&app).sort, PageSort::Name);
        assert_eq!(names(&app, page(&app)), vec!["alpha", "mid", "zeta"]);
        update(&mut app, Action::MsPageCycleSort);
        assert_eq!(page(&app).sort, PageSort::Target);
        assert_eq!(
            names(&app, page(&app)),
            vec!["zeta", "mid", "alpha"],
            "undated sorts last"
        );
        update(&mut app, Action::MsPageCycleSort);
        assert_eq!(page(&app).sort, PageSort::Activity);
    }

    #[test]
    fn typing_filters_on_name_or_project_and_resets_the_cursor() {
        let mut app = App::new();
        app.milestones = vec![
            milestone("alpha", "open"),
            milestone("beta", "open"),
            other_project("gamma"),
        ];
        update(&mut app, Action::OpenMilestonePage);
        update(&mut app, Action::MsPageDown); // cursor -> 1
        update(&mut app, Action::MsPageInput('b'));
        update(&mut app, Action::MsPageInput('e'));
        let s = page(&app);
        assert_eq!(s.query, "be");
        assert_eq!(s.cursor, 0, "typing resets the cursor");
        assert_eq!(names(&app, s), vec!["beta"]);

        // The project key is searchable too.
        update(&mut app, Action::MsPageBackspace);
        update(&mut app, Action::MsPageBackspace);
        update(&mut app, Action::MsPageInput('l'));
        update(&mut app, Action::MsPageInput('m'));
        assert_eq!(names(&app, page(&app)), vec!["gamma"]);
    }

    #[test]
    fn cursor_down_is_clamped_to_the_visible_rows() {
        let mut app = App::new();
        app.milestones = vec![milestone("alpha", "open"), milestone("beta", "open")];
        update(&mut app, Action::OpenMilestonePage);
        update(&mut app, Action::MsPageInput('a'));
        update(&mut app, Action::MsPageInput('l')); // only "alpha" matches
        update(&mut app, Action::MsPageDown);
        assert_eq!(page(&app).cursor, 0);
    }

    #[test]
    fn enter_scopes_the_board_to_the_focused_milestone_and_its_project() {
        let mut app = App::new();
        app.milestones = vec![milestone("M1", "open"), other_project("LM-only")];
        update(&mut app, Action::OpenMilestonePage);
        update(&mut app, Action::MsPageDown); // -> the LM milestone
        let cmd = update(&mut app, Action::MsPageSelect);
        assert_eq!(app.scope.project.as_deref(), Some("LM"));
        assert_eq!(app.scope.milestone.as_deref(), Some("LM-only"));
        assert!(matches!(app.mode, Mode::Normal), "page closes on select");
        assert!(matches!(cmd, Some(Command::SetScope)));
    }

    #[test]
    fn edit_and_new_carry_the_focused_milestones_project() {
        let mut app = App::new();
        app.milestones = vec![other_project("LM-only")];
        update(&mut app, Action::OpenMilestonePage);
        match update(&mut app, Action::MsPageEdit).unwrap() {
            Command::EditMilestone { project, name } => {
                assert_eq!((project.as_str(), name.as_str()), ("LM", "LM-only"));
            }
            c => panic!("{c:?}"),
        }
        match update(&mut app, Action::MsPageNew).unwrap() {
            Command::NewMilestone { project } => assert_eq!(project.as_deref(), Some("LM")),
            c => panic!("{c:?}"),
        }
    }

    #[test]
    fn c_cycles_the_focused_milestone_through_its_statuses() {
        let mut app = App::new();
        app.milestones = vec![milestone("M1", "open")];
        update(&mut app, Action::OpenMilestonePage);
        // The page shows the *current* status, so each press emits the next
        // one; the reload after the write is what advances the cycle.
        for (from, to) in [
            ("open", "completed"),
            ("completed", "cancelled"),
            ("cancelled", "open"),
        ] {
            app.milestones[0].status = from.into();
            if let Mode::MilestonePage(s) = &mut app.mode {
                s.filter = StatusFilter::All;
            }
            match update(&mut app, Action::MsPageCycleStatus).unwrap() {
                Command::SetMilestoneStatus { name, status, .. } => {
                    assert_eq!(name, "M1");
                    assert_eq!(status, to, "from {from}");
                }
                c => panic!("{c:?}"),
            }
        }
    }

    #[test]
    fn page_actions_on_an_empty_list_are_noops() {
        let mut app = App::new();
        update(&mut app, Action::OpenMilestonePage);
        assert!(update(&mut app, Action::MsPageEdit).is_none());
        assert!(update(&mut app, Action::MsPageSelect).is_none());
        assert!(update(&mut app, Action::MsPageCycleStatus).is_none());
        // New still works — it is how you get your first milestone.
        assert!(update(&mut app, Action::MsPageNew).is_some());
    }

    fn names<'a>(app: &'a App, s: &MilestonePageState) -> Vec<&'a str> {
        page_rows(app, s)
            .into_iter()
            .map(|i| app.milestones[i].name.as_str())
            .collect()
    }

    #[test]
    fn archive_asks_first_then_archives_the_captured_key() {
        let mut app = App::new();
        app.cards = vec![card("PULSE-9", "backlog", 1000.0)];
        assert!(update(&mut app, Action::ArchiveRequest).is_none());
        assert!(matches!(&app.mode, Mode::ConfirmArchive(k) if k == "PULSE-9"));
        match update(&mut app, Action::Archive) {
            Some(Command::Archive { key }) => assert_eq!(key, "PULSE-9"),
            c => panic!("{c:?}"),
        }
        assert!(matches!(app.mode, Mode::Normal));
        // Declining leaves the card alone and returns to the board.
        update(&mut app, Action::ArchiveRequest);
        assert!(update(&mut app, Action::Cancel).is_none());
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn number_jump_lands_even_on_empty_columns_and_half_pages_clamp() {
        let mut app = App::new();
        app.cards = (0..8)
            .map(|i| card(&format!("PULSE-{i}"), "backlog", 1000.0 + i as f64))
            .collect();
        // 3 = the (empty) blocked column, where `n` would create.
        update(&mut app, Action::FocusColumn(2));
        assert_eq!(app.focus.column, ColumnId::Blocked);
        update(&mut app, Action::FocusColumn(0));
        assert_eq!(app.focus.column, ColumnId::Backlog);
        update(&mut app, Action::PageMove(Direction::Down));
        assert_eq!(app.focus.card_idx, 5);
        update(&mut app, Action::PageMove(Direction::Down));
        assert_eq!(app.focus.card_idx, 7, "clamps to the last card");
        update(&mut app, Action::PageMove(Direction::Up));
        assert_eq!(app.focus.card_idx, 2);
    }

    #[test]
    fn milestone_page_jumps_pages_and_cycles_the_filter_both_ways() {
        let mut app = App::new();
        app.milestones = (0..15)
            .map(|i| milestone(&format!("M{i}"), "open"))
            .collect();
        update(&mut app, Action::OpenMilestonePage);
        update(&mut app, Action::MsPagePage(Direction::Down));
        let cursor = |app: &App| match &app.mode {
            Mode::MilestonePage(s) => s.cursor,
            _ => panic!("not on the page"),
        };
        assert_eq!(cursor(&app), 10);
        update(&mut app, Action::MsPageBottom);
        assert_eq!(cursor(&app), 14);
        update(&mut app, Action::MsPageTop);
        assert_eq!(cursor(&app), 0);
        // Shift-Tab walks the buckets backwards: open → all.
        update(&mut app, Action::MsPageCycleFilterBack);
        match &app.mode {
            Mode::MilestonePage(s) => assert_eq!(s.filter, StatusFilter::All),
            _ => panic!("not on the page"),
        }
    }

    fn project(key: &str, archived: bool) -> ProjectRef {
        ProjectRef {
            key: key.into(),
            name: key.to_lowercase(),
            description: String::new(),
            archived,
            total: 0,
            done: 0,
            milestones: 0,
            last_activity: DateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn project_archive_confirms_returns_to_the_page_and_unscopes_the_board() {
        let mut app = App::new();
        app.projects = vec![project("PULSE", false)];
        app.scope.set_project(Some("PULSE".into()));
        update(&mut app, Action::OpenProjectPage);
        assert!(update(&mut app, Action::ProjPageArchiveRequest).is_none());
        assert!(matches!(
            &app.mode,
            Mode::ConfirmProjectArchive { key, archived: true, .. } if key == "PULSE"
        ));
        // Declining lands back on the page, not the board.
        update(&mut app, Action::Cancel);
        assert!(matches!(app.mode, Mode::ProjectPage(_)));
        // Confirming emits the command and clears the now-stale board scope.
        update(&mut app, Action::ProjPageArchiveRequest);
        match update(&mut app, Action::ProjPageArchive) {
            Some(Command::SetProjectArchived { key, archived }) => {
                assert_eq!(key, "PULSE");
                assert!(archived);
            }
            c => panic!("{c:?}"),
        }
        assert_eq!(app.scope.project, None, "archived scope must clear");
        assert!(matches!(app.mode, Mode::ProjectPage(_)));
    }

    #[test]
    fn selecting_a_project_scopes_the_board_but_archived_ones_refuse() {
        let mut app = App::new();
        app.projects = vec![project("PULSE", false), project("OLD", true)];
        update(&mut app, Action::OpenProjectPage);
        assert!(matches!(
            update(&mut app, Action::ProjPageSelect),
            Some(Command::SetScope)
        ));
        assert_eq!(app.scope.project.as_deref(), Some("PULSE"));
        assert!(matches!(app.mode, Mode::Normal));

        // The archived bucket can browse but not scope.
        update(&mut app, Action::OpenProjectPage);
        update(&mut app, Action::ProjPageCycleFilter); // active → archived
        assert!(update(&mut app, Action::ProjPageSelect).is_none());
        assert!(app.status_msg.is_some(), "refusal must explain itself");
        assert!(matches!(app.mode, Mode::ProjectPage(_)));
    }

    #[test]
    fn project_rows_filter_query_and_sort() {
        let mut app = App::new();
        app.projects = vec![
            project("TIDE", false),
            project("PULSE", false),
            project("OLD", true),
        ];
        let s = ProjectPageState::default();
        let keys = |app: &App, s: &ProjectPageState| -> Vec<String> {
            project_rows(app, s)
                .into_iter()
                .map(|i| app.projects[i].key.clone())
                .collect()
        };
        assert_eq!(keys(&app, &s), ["TIDE", "PULSE"], "active bucket");
        let s = ProjectPageState {
            query: "pul".into(),
            ..Default::default()
        };
        assert_eq!(keys(&app, &s), ["PULSE"], "query matches key");
        let s = ProjectPageState {
            filter: ProjFilter::All,
            sort: ProjSort::Name,
            ..Default::default()
        };
        assert_eq!(keys(&app, &s), ["OLD", "PULSE", "TIDE"], "name sort");
    }

    fn act(key: &str, kind: &str, message: &str) -> ActivityRef {
        ActivityRef {
            issue_key: key.into(),
            title: "t".into(),
            project: "CLI".into(),
            kind: kind.into(),
            message: message.into(),
            actor: None,
            ts: DateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn closes_means_a_status_move_that_landed_in_done() {
        assert!(act("K-1", "status", "backlog → done").closes());
        assert!(act("K-1", "status", "in-review → done: shipped it").closes());
        assert!(!act("K-1", "status", "done → backlog").closes());
        assert!(!act("K-1", "status", "backlog → in-progress").closes());
        assert!(!act("K-1", "archive", "archived").closes());
        assert!(!act("K-1", "log", "note about done things").closes());
    }

    #[test]
    fn activity_enter_jumps_to_the_issue_or_explains_why_not() {
        let mut app = App::new();
        app.cards = vec![card("PULSE-7", "in-progress", 1000.0)];
        app.activity = vec![
            act("PULSE-7", "status", "backlog → in-progress"),
            act("PULSE-9", "archive", "archived"),
        ];
        update(&mut app, Action::OpenActivityPage);
        // Enter on a visible issue lands the board cursor on it.
        assert!(update(&mut app, Action::ActPageSelect).is_none());
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.focus.column, ColumnId::InProgress);
        // Enter on an archived issue stays on the page and says why.
        update(&mut app, Action::OpenActivityPage);
        update(&mut app, Action::ActPageDown);
        update(&mut app, Action::ActPageSelect);
        assert!(matches!(app.mode, Mode::ActivityPage(_)));
        assert!(app.status_msg.as_deref().unwrap_or("").contains("PULSE-9"));
    }

    #[test]
    fn activity_rows_filter_by_kind_and_query() {
        let mut app = App::new();
        app.activity = vec![
            act("PULSE-1", "status", "backlog → done"),
            act("PULSE-2", "status", "backlog → in-progress"),
            act("TIDE-3", "log", "root cause found"),
        ];
        let keys = |app: &App, s: &ActivityPageState| -> Vec<String> {
            activity_rows(app, s)
                .into_iter()
                .map(|i| app.activity[i].issue_key.clone())
                .collect()
        };
        let s = ActivityPageState::default();
        assert_eq!(keys(&app, &s).len(), 3);
        let s = ActivityPageState {
            filter: ActFilter::Closed,
            ..Default::default()
        };
        assert_eq!(keys(&app, &s), ["PULSE-1"]);
        let s = ActivityPageState {
            query: "root".into(),
            ..Default::default()
        };
        assert_eq!(keys(&app, &s), ["TIDE-3"]);
    }

    #[test]
    fn undo_reverses_moves_reorders_and_archives_in_lifo_order() {
        let mut app = App::new();
        app.cards = vec![
            card("PULSE-1", "backlog", 1000.0),
            card("PULSE-2", "backlog", 2000.0),
        ];
        // A Space-move records its inverse…
        update(&mut app, Action::BeginMove);
        assert!(matches!(
            update(&mut app, Action::MoveTo("done".into())),
            Some(Command::MoveIssue { .. })
        ));
        // …an archive records its own…
        update(&mut app, Action::ArchiveRequest);
        update(&mut app, Action::Archive);
        // …and undo pops them newest-first.
        match update(&mut app, Action::Undo) {
            Some(Command::Unarchive { key }) => assert_eq!(key, "PULSE-1"),
            c => panic!("expected unarchive, got {c:?}"),
        }
        match update(&mut app, Action::Undo) {
            Some(Command::MoveIssue { key, status }) => {
                assert_eq!(key, "PULSE-1");
                assert_eq!(status, "backlog", "moves back to where it was");
            }
            c => panic!("expected move-back, got {c:?}"),
        }
        // Empty stack refuses politely.
        assert!(update(&mut app, Action::Undo).is_none());
        assert_eq!(app.status_msg.as_deref(), Some("nothing to undo"));
    }

    #[test]
    fn undo_of_a_reorder_swaps_the_same_pair_again() {
        let mut app = App::new();
        app.cards = vec![
            card("PULSE-1", "backlog", 1000.0),
            card("PULSE-2", "backlog", 2000.0),
        ];
        assert!(matches!(
            update(&mut app, Action::MoveIssueDir(Direction::Down)),
            Some(Command::Reorder { .. })
        ));
        match update(&mut app, Action::Undo) {
            Some(Command::Reorder { key, other }) => {
                assert_eq!(key, "PULSE-1");
                assert_eq!(other, "PULSE-2");
            }
            c => panic!("expected reorder, got {c:?}"),
        }
    }

    #[test]
    fn unseen_counts_only_news_from_others() {
        let mut app = App::new();
        app.self_actor = Some("claude".into());
        let now = chrono::Utc::now();
        let at = |secs_ago: i64, actor: Option<&str>| {
            let mut e = act("PULSE-9", "status", "backlog → done");
            e.ts = now - chrono::Duration::seconds(secs_ago);
            e.actor = actor.map(String::from);
            e
        };
        app.activity = vec![
            at(10, Some("alex")),   // new, someone else → counts
            at(20, Some("claude")), // new, but mine → not news
            at(30, None),           // new, unattributed → counts
            at(300, Some("alex")),  // old → already seen
        ];
        app.last_seen = Some(now - chrono::Duration::seconds(60));
        assert_eq!(app.unseen_count(), 2);
        // With no last-seen, everything foreign is unread.
        app.last_seen = None;
        assert_eq!(app.unseen_count(), 3);
        // Reading the mailbox clears it.
        app.mark_seen();
        assert_eq!(app.unseen_count(), 0);
    }

    #[test]
    fn the_next_action_dismisses_a_stale_status_message() {
        let mut app = App::new();
        // A refused action leaves its explanation…
        update(&mut app, Action::EditScope);
        assert!(app.status_msg.is_some());
        // …and the very next action clears it instead of letting it haunt
        // the footer for the rest of the session.
        update(&mut app, Action::FocusMove(Direction::Down));
        assert!(app.status_msg.is_none());
        // An action that refuses re-sets its own message after the clear.
        update(&mut app, Action::EditScope);
        assert!(app.status_msg.is_some());
    }
}
