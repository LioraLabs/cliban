//! UI actions (input to `app::update`) and Commands (side-effects). Pure data.
use crate::app::Scope;

#[derive(Debug, Clone, Copy)]
pub enum Direction { Up, Down, Left, Right }

#[derive(Debug, Clone)]
pub enum Action {
    FocusMove(Direction), JumpToTop, JumpToBottom,
    ToggleHelp, QuitRequest, Quit, Cancel, Refresh,
    OpenDetail, EditCard, EditScope, NewIssue, NewMilestone, TagMilestone, Archive,
    BeginMove, MoveTo(String),
    OpenProjectPicker, OpenMilestonePicker, CycleMilestoneFilter, OpenMilestoneOverlay,
    SetScope(Scope),
    PickerInput(char), PickerBackspace, PickerUp, PickerDown, PickerConfirm,
    OpenFuzzyFind, FuzzyInput(char), FuzzyBackspace, FuzzyUp, FuzzyDown, FuzzyConfirm,
    OverlayUp, OverlayDown, OverlayEdit,
}

#[derive(Debug, Clone)]
pub enum Command {
    MoveIssue { key: String, status: String },
    Archive { key: String },
    TagMilestone { key: String, milestone: Option<String> },
    SetScope, Reload,
    EditIssue { key: String },
    NewIssue { status: String },
    EditMilestone { name: String },
    NewMilestone,
    EditProject,
}
