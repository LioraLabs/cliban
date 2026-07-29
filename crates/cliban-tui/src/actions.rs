//! UI actions (input to `app::update`) and Commands (side-effects). Pure data.
use crate::app::{ColumnId, Scope};

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub enum Action {
    FocusMove(Direction),
    /// Jump straight to the Nth visible column (the `1`-`5` keys).
    FocusColumn(usize),
    /// Half-page the focus within the current column (Ctrl-d/u, PgDn/PgUp).
    PageMove(Direction),
    /// Mouse: land the board cursor on a specific card (idx clamped).
    FocusCard(ColumnId, usize),
    /// Mouse: set the active page's cursor to an absolute row.
    PageCursor(usize),
    JumpToTop,
    JumpToBottom,
    ToggleHelp,
    QuitRequest,
    Quit,
    Cancel,
    Refresh,
    OpenDetail,
    /// `b` in the detail popup: jump to the first still-open blocker.
    JumpToBlocker,
    EditCard,
    EditScope,
    NewIssue,
    NewMilestone,
    // The new-dialog form (n/N open it; these drive it).
    NewDialogInput(char),
    NewDialogBackspace,
    NewDialogNextField,
    NewDialogPrevField,
    NewDialogConfirm,
    TagMilestone,
    /// `a`: ask before archiving (opens the confirm dialog).
    ArchiveRequest,
    /// The confirmed archive (from the dialog).
    Archive,
    /// `u`: reverse the last move, reorder, or archive of this session.
    Undo,
    BeginMove,
    MoveTo(String),
    MoveIssueDir(Direction),
    OpenMilestonePicker,
    CycleMilestoneFilter,
    OpenMilestonePage,
    SetScope(Scope),
    PickerInput(char),
    PickerBackspace,
    PickerUp,
    PickerDown,
    PickerConfirm,
    OpenFuzzyFind,
    FuzzyInput(char),
    FuzzyBackspace,
    FuzzyUp,
    FuzzyDown,
    FuzzyConfirm,
    // Milestone page (`m`).
    MsPageUp,
    MsPageDown,
    /// Half-page the cursor (PgDn/PgUp).
    MsPagePage(Direction),
    MsPageTop,
    MsPageBottom,
    MsPageInput(char),
    MsPageBackspace,
    /// Enter: scope the board to the focused milestone and close the page.
    MsPageSelect,
    MsPageEdit,
    MsPageNew,
    /// Cycle the focused milestone's own status (open → completed → cancelled).
    MsPageCycleStatus,
    /// Cycle which status bucket the page lists (Tab; Shift-Tab reverses).
    MsPageCycleFilter,
    MsPageCycleFilterBack,
    MsPageCycleSort,
    // Project page (`p`) — the same grammar as the milestone page.
    OpenProjectPage,
    ProjPageUp,
    ProjPageDown,
    ProjPagePage(Direction),
    ProjPageTop,
    ProjPageBottom,
    ProjPageInput(char),
    ProjPageBackspace,
    /// Enter: scope the board to the focused project and close the page.
    ProjPageSelect,
    ProjPageEdit,
    ProjPageNew,
    /// `A`: ask before flipping the focused project's archived flag.
    ProjPageArchiveRequest,
    /// The confirmed archive/unarchive (from the dialog).
    ProjPageArchive,
    ProjPageCycleFilter,
    ProjPageCycleFilterBack,
    ProjPageCycleSort,
    // Activity page (`A`) — the mailbox.
    OpenActivityPage,
    ActPageUp,
    ActPageDown,
    ActPagePage(Direction),
    ActPageTop,
    ActPageBottom,
    ActPageInput(char),
    ActPageBackspace,
    /// Enter: jump the board cursor to the focused entry's issue.
    ActPageSelect,
    ActPageCycleFilter,
    ActPageCycleFilterBack,
}

#[derive(Debug, Clone)]
pub enum Command {
    MoveIssue {
        key: String,
        status: String,
    },
    /// Swap the board positions of two issues in the same column (J/K reorder).
    Reorder {
        key: String,
        other: String,
    },
    Archive {
        key: String,
    },
    /// Undo's inverse of an archive.
    Unarchive {
        key: String,
    },
    TagMilestone {
        key: String,
        milestone: Option<String>,
    },
    SetScope,
    Reload,
    EditIssue {
        key: String,
    },
    /// Create straight from the new-issue dialog — everything the write needs
    /// was resolved when the dialog opened, so no editor round-trip.
    CreateIssue {
        project: String,
        status: String,
        title: String,
        /// Inherited from the board's milestone scope; empty means none.
        milestone: String,
    },
    EditMilestone {
        project: String,
        name: String,
    },
    CreateMilestone {
        project: String,
        name: String,
        /// `YYYY-MM-DD`, already validated by the dialog; empty means none.
        target: String,
    },
    SetMilestoneStatus {
        project: String,
        name: String,
        status: String,
    },
    EditProject {
        key: String,
    },
    CreateProject {
        key: String,
        name: String,
    },
    SetProjectArchived {
        key: String,
        archived: bool,
    },
}
