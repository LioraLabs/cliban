//! crossterm KeyEvent → Action, dispatched on Mode. Pure.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::actions::{Action, Direction};
use crate::app::{App, Mode};

fn status_for_letter(c: char) -> Option<&'static str> {
    match c { 'b' => Some("backlog"), 'i' => Some("in-progress"), 'k' => Some("blocked"),
              'r' => Some("in-review"), 'd' => Some("done"), _ => None }
}

pub fn map_key(key: KeyEvent, app: &mut App) -> Option<Action> {
    match &app.mode {
        Mode::Normal => map_normal(key, app),
        Mode::AwaitingMove => map_awaiting_move(key),
        Mode::Help => Some(Action::Cancel),
        Mode::Detail(_) => match key.code { KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => Some(Action::Cancel), _ => None },
        Mode::ConfirmQuit => map_confirm_quit(key),
        Mode::ProjectPicker(_) | Mode::MilestonePicker(_) => map_picker(key),
        Mode::FuzzyFind(_) => map_fuzzy(key),
        Mode::MilestoneOverlay(_) => map_overlay(key),
    }
}

fn map_normal(key: KeyEvent, app: &mut App) -> Option<Action> {
    let was_g = app.pending_g;
    let is_g = matches!((key.code, key.modifiers), (KeyCode::Char('g'), KeyModifiers::NONE));
    if was_g && !is_g { app.pending_g = false; }
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => Some(Action::QuitRequest),
        // Capital H/J/K/L move the focused ISSUE: H/L across columns (status),
        // J/K reorder within the column. (Lowercase moves the cursor.)
        (KeyCode::Char('H'), _) => Some(Action::MoveIssueDir(Direction::Left)),
        (KeyCode::Char('L'), _) => Some(Action::MoveIssueDir(Direction::Right)),
        (KeyCode::Char('J'), _) => Some(Action::MoveIssueDir(Direction::Down)),
        (KeyCode::Char('K'), _) => Some(Action::MoveIssueDir(Direction::Up)),
        (KeyCode::Char('h'), KeyModifiers::NONE) | (KeyCode::Left, _) => Some(Action::FocusMove(Direction::Left)),
        (KeyCode::Char('l'), KeyModifiers::NONE) | (KeyCode::Right, _) => Some(Action::FocusMove(Direction::Right)),
        (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, _) => Some(Action::FocusMove(Direction::Down)),
        (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, _) => Some(Action::FocusMove(Direction::Up)),
        (KeyCode::Tab, _) => Some(Action::FocusMove(Direction::Right)),
        (KeyCode::BackTab, _) => Some(Action::FocusMove(Direction::Left)),
        (KeyCode::Char('g'), KeyModifiers::NONE) => { if was_g { app.pending_g = false; Some(Action::JumpToTop) } else { app.pending_g = true; None } }
        (KeyCode::Char('G'), _) => Some(Action::JumpToBottom),
        (KeyCode::Enter, _) => Some(Action::OpenDetail),
        (KeyCode::Char('e'), KeyModifiers::NONE) => Some(Action::EditCard),
        (KeyCode::Char('E'), _) => Some(Action::EditScope),
        (KeyCode::Char('n'), KeyModifiers::NONE) => Some(Action::NewIssue),
        (KeyCode::Char('N'), _) => Some(Action::NewMilestone),
        (KeyCode::Char('t'), KeyModifiers::NONE) => Some(Action::TagMilestone),
        (KeyCode::Char(' '), _) => Some(Action::BeginMove),
        (KeyCode::Char('a'), KeyModifiers::NONE) => Some(Action::Archive),
        (KeyCode::Char('m'), KeyModifiers::NONE) => Some(Action::OpenMilestoneOverlay),
        (KeyCode::Char('M'), _) => Some(Action::CycleMilestoneFilter),
        (KeyCode::Char('p'), KeyModifiers::NONE) => Some(Action::OpenProjectPicker),
        (KeyCode::Char('/'), _) => Some(Action::OpenFuzzyFind),
        (KeyCode::Char('r'), KeyModifiers::NONE) => Some(Action::Refresh),
        (KeyCode::Char('?'), _) => Some(Action::ToggleHelp),
        (KeyCode::Esc, _) => Some(Action::QuitRequest),
        _ => None,
    }
}

fn map_awaiting_move(key: KeyEvent) -> Option<Action> {
    if let KeyCode::Char(c) = key.code {
        if let Some(s) = status_for_letter(c) { return Some(Action::MoveTo(s.to_string())); }
    }
    Some(Action::Cancel)
}

fn map_confirm_quit(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(Action::Quit),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Some(Action::Cancel),
        _ => None,
    }
}

fn map_picker(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::PickerConfirm),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::PickerBackspace),
        (KeyCode::Up, _) => Some(Action::PickerUp),
        (KeyCode::Down, _) => Some(Action::PickerDown),
        (KeyCode::Char('j'), KeyModifiers::NONE) => Some(Action::PickerDown),
        (KeyCode::Char('k'), KeyModifiers::NONE) => Some(Action::PickerUp),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => Some(Action::PickerInput(c)),
        _ => None,
    }
}

fn map_fuzzy(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::FuzzyConfirm),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::FuzzyBackspace),
        (KeyCode::Up, _) => Some(Action::FuzzyUp),
        (KeyCode::Down, _) => Some(Action::FuzzyDown),
        (KeyCode::Char('j'), KeyModifiers::NONE) => Some(Action::FuzzyDown),
        (KeyCode::Char('k'), KeyModifiers::NONE) => Some(Action::FuzzyUp),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => Some(Action::FuzzyInput(c)),
        _ => None,
    }
}

fn map_overlay(key: KeyEvent) -> Option<Action> {
    // Mirrors `map_picker` so the milestone overlay scrolls and filters like
    // the project picker: j/k (and arrows) navigate, Enter selects, Esc
    // cancels, Backspace edits the query, and any other printable char types
    // into the filter. `E` (shift+e) stays reserved for editing the focused
    // milestone — the filter is case-insensitive so a lowercase `e` still
    // filters and matches names containing `E`.
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::OverlaySelect),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::OverlayBackspace),
        (KeyCode::Down, _) => Some(Action::OverlayDown),
        (KeyCode::Up, _) => Some(Action::OverlayUp),
        (KeyCode::Char('j'), KeyModifiers::NONE) => Some(Action::OverlayDown),
        (KeyCode::Char('k'), KeyModifiers::NONE) => Some(Action::OverlayUp),
        (KeyCode::Char('E'), _) => Some(Action::OverlayEdit),
        (KeyCode::Char('A'), _) => Some(Action::OverlayToggleAll),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => Some(Action::OverlayInput(c)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ke(c: KeyCode) -> KeyEvent { KeyEvent::new(c, KeyModifiers::NONE) }

    #[test]
    fn space_begins_move_then_letter_resolves_status() {
        let mut app = App::new();
        assert!(matches!(map_key(ke(KeyCode::Char(' ')), &mut app), Some(Action::BeginMove)));
        app.mode = Mode::AwaitingMove;
        match map_key(ke(KeyCode::Char('d')), &mut app) { Some(Action::MoveTo(s)) => assert_eq!(s, "done"), o => panic!("{o:?}") }
    }

    #[test]
    fn overlay_types_into_filter_but_reserves_nav_and_edit() {
        use crate::app::{MilestoneOverlayState, Mode};
        let mut app = App::new();
        app.mode = Mode::MilestoneOverlay(MilestoneOverlayState { items: vec![], cursor: 0, query: String::new(), show_all: false });
        // printable char → filter input
        assert!(matches!(map_key(ke(KeyCode::Char('a')), &mut app), Some(Action::OverlayInput('a'))));
        // j/k still navigate (mirrors the project picker)
        assert!(matches!(map_key(ke(KeyCode::Char('j')), &mut app), Some(Action::OverlayDown)));
        assert!(matches!(map_key(ke(KeyCode::Char('k')), &mut app), Some(Action::OverlayUp)));
        // capital E stays reserved for editing the focused milestone
        let e = KeyEvent::new(KeyCode::Char('E'), KeyModifiers::SHIFT);
        assert!(matches!(map_key(e, &mut app), Some(Action::OverlayEdit)));
        // capital A toggles open-only vs all statuses; lowercase a still filters
        let a_cap = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
        assert!(matches!(map_key(a_cap, &mut app), Some(Action::OverlayToggleAll)));
        assert!(matches!(map_key(ke(KeyCode::Char('a')), &mut app), Some(Action::OverlayInput('a'))));
        assert!(matches!(map_key(ke(KeyCode::Backspace), &mut app), Some(Action::OverlayBackspace)));
        assert!(matches!(map_key(ke(KeyCode::Enter), &mut app), Some(Action::OverlaySelect)));
        assert!(matches!(map_key(ke(KeyCode::Esc), &mut app), Some(Action::Cancel)));
    }

    #[test]
    fn normal_keys_dispatch_to_cliban_actions() {
        let mut app = App::new();
        assert!(matches!(map_key(ke(KeyCode::Char('n')), &mut app), Some(Action::NewIssue)));
        assert!(matches!(map_key(ke(KeyCode::Char('N')), &mut app), Some(Action::NewMilestone)));
        assert!(matches!(map_key(ke(KeyCode::Char('t')), &mut app), Some(Action::TagMilestone)));
        assert!(matches!(map_key(ke(KeyCode::Char('a')), &mut app), Some(Action::Archive)));
        assert!(matches!(map_key(ke(KeyCode::Char('m')), &mut app), Some(Action::OpenMilestoneOverlay)));
        assert!(matches!(map_key(ke(KeyCode::Char('M')), &mut app), Some(Action::CycleMilestoneFilter)));
        assert!(matches!(map_key(ke(KeyCode::Char('e')), &mut app), Some(Action::EditCard)));
        assert!(matches!(map_key(ke(KeyCode::Char('E')), &mut app), Some(Action::EditScope)));
        assert!(matches!(map_key(ke(KeyCode::Char('p')), &mut app), Some(Action::OpenProjectPicker)));
        assert!(matches!(map_key(ke(KeyCode::Char('/')), &mut app), Some(Action::OpenFuzzyFind)));
        assert!(matches!(map_key(ke(KeyCode::Char('r')), &mut app), Some(Action::Refresh)));
        assert!(matches!(map_key(ke(KeyCode::Char('q')), &mut app), Some(Action::QuitRequest)));
        assert!(matches!(map_key(ke(KeyCode::Enter), &mut app), Some(Action::OpenDetail)));
    }
}
