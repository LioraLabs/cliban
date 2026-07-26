//! UI actions (input to `app::update`) and Commands (side-effects). Pure data.
use crate::app::Scope;

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
    JumpToTop,
    JumpToBottom,
    ToggleHelp,
    QuitRequest,
    Quit,
    Cancel,
    Refresh,
    OpenDetail,
    EditCard,
    EditScope,
    NewIssue,
    NewMilestone,
    TagMilestone,
    Archive,
    BeginMove,
    MoveTo(String),
    MoveIssueDir(Direction),
    OpenProjectPicker,
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
    MsPageInput(char),
    MsPageBackspace,
    /// Enter: scope the board to the focused milestone and close the page.
    MsPageSelect,
    MsPageEdit,
    MsPageNew,
    /// Cycle the focused milestone's own status (open → completed → cancelled).
    MsPageCycleStatus,
    /// Cycle which status bucket the page lists.
    MsPageCycleFilter,
    MsPageCycleSort,
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
    TagMilestone {
        key: String,
        milestone: Option<String>,
    },
    SetScope,
    Reload,
    EditIssue {
        key: String,
    },
    NewIssue {
        status: String,
    },
    EditMilestone {
        project: String,
        name: String,
    },
    /// `None` falls back to the board's scoped project (and errors if unscoped).
    NewMilestone {
        project: Option<String>,
    },
    SetMilestoneStatus {
        project: String,
        name: String,
        status: String,
    },
    EditProject,
}
