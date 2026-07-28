//! crossterm KeyEvent → Action, dispatched on Mode. Pure.
use crate::actions::{Action, Direction};
use crate::app::{App, Mode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn status_for_letter(c: char) -> Option<&'static str> {
    match c {
        'b' => Some("backlog"),
        'i' => Some("in-progress"),
        'k' => Some("blocked"),
        'r' => Some("in-review"),
        'd' => Some("done"),
        _ => None,
    }
}

pub fn map_key(key: KeyEvent, app: &mut App) -> Option<Action> {
    // Ctrl-C is the universal "get me out" chord: it quits from any mode,
    // no confirm — the dialog is for accidental q, not for deliberate SIGINT
    // muscle memory.
    if matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL)
    ) {
        return Some(Action::Quit);
    }
    match &app.mode {
        Mode::Normal => map_normal(key, app),
        Mode::AwaitingMove => map_awaiting_move(key),
        Mode::Help => Some(Action::Cancel),
        Mode::Detail(_) => match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => Some(Action::Cancel),
            // Edit straight from the detail view; the popup stays open and
            // re-renders the updated card when the editor returns.
            KeyCode::Char('e') => Some(Action::EditCard),
            _ => None,
        },
        Mode::ConfirmQuit => map_confirm_quit(key),
        Mode::ProjectPicker(_) | Mode::MilestonePicker(_) => map_picker(key),
        Mode::FuzzyFind(_) => map_fuzzy(key),
        Mode::MilestonePage(_) => map_milestone_page(key),
    }
}

fn map_normal(key: KeyEvent, app: &mut App) -> Option<Action> {
    let was_g = app.pending_g;
    let is_g = matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('g'), KeyModifiers::NONE)
    );
    if was_g && !is_g {
        app.pending_g = false;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => Some(Action::QuitRequest),
        // Capital H/J/K/L move the focused ISSUE: H/L across columns (status),
        // J/K reorder within the column. (Lowercase moves the cursor.)
        (KeyCode::Char('H'), _) => Some(Action::MoveIssueDir(Direction::Left)),
        (KeyCode::Char('L'), _) => Some(Action::MoveIssueDir(Direction::Right)),
        (KeyCode::Char('J'), _) => Some(Action::MoveIssueDir(Direction::Down)),
        (KeyCode::Char('K'), _) => Some(Action::MoveIssueDir(Direction::Up)),
        (KeyCode::Char('h'), KeyModifiers::NONE) | (KeyCode::Left, _) => {
            Some(Action::FocusMove(Direction::Left))
        }
        (KeyCode::Char('l'), KeyModifiers::NONE) | (KeyCode::Right, _) => {
            Some(Action::FocusMove(Direction::Right))
        }
        (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, _) => {
            Some(Action::FocusMove(Direction::Down))
        }
        (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, _) => {
            Some(Action::FocusMove(Direction::Up))
        }
        (KeyCode::Tab, _) => Some(Action::FocusMove(Direction::Right)),
        (KeyCode::BackTab, _) => Some(Action::FocusMove(Direction::Left)),
        (KeyCode::Char('g'), KeyModifiers::NONE) => {
            if was_g {
                app.pending_g = false;
                Some(Action::JumpToTop)
            } else {
                app.pending_g = true;
                None
            }
        }
        (KeyCode::Char('G'), _) => Some(Action::JumpToBottom),
        (KeyCode::Enter, _) => Some(Action::OpenDetail),
        (KeyCode::Char('e'), KeyModifiers::NONE) => Some(Action::EditCard),
        (KeyCode::Char('E'), _) => Some(Action::EditScope),
        (KeyCode::Char('n'), KeyModifiers::NONE) => Some(Action::NewIssue),
        (KeyCode::Char('N'), _) => Some(Action::NewMilestone),
        (KeyCode::Char('t'), KeyModifiers::NONE) => Some(Action::TagMilestone),
        (KeyCode::Char(' '), _) => Some(Action::BeginMove),
        (KeyCode::Char('a'), KeyModifiers::NONE) => Some(Action::Archive),
        (KeyCode::Char('m'), KeyModifiers::NONE) => Some(Action::OpenMilestonePage),
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
        if let Some(s) = status_for_letter(c) {
            return Some(Action::MoveTo(s.to_string()));
        }
    }
    Some(Action::Cancel)
}

fn map_confirm_quit(key: KeyEvent) -> Option<Action> {
    match key.code {
        // Enter confirms (the dialog's default answer), and `q` again means
        // a quick `q q` quits without reaching for `y`.
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('q') | KeyCode::Enter => {
            Some(Action::Quit)
        }
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

fn map_milestone_page(key: KeyEvent) -> Option<Action> {
    // Typing filters, as on the project picker — so every page *command* is a
    // capital letter or a named key, leaving the whole lowercase alphabet free
    // for the query. The filter is case-insensitive, so `e`, `n`, `s` and `c`
    // still match names containing those letters.
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::MsPageSelect),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::MsPageBackspace),
        (KeyCode::Down, _) => Some(Action::MsPageDown),
        (KeyCode::Up, _) => Some(Action::MsPageUp),
        (KeyCode::Char('j'), KeyModifiers::NONE) => Some(Action::MsPageDown),
        (KeyCode::Char('k'), KeyModifiers::NONE) => Some(Action::MsPageUp),
        (KeyCode::Tab, _) | (KeyCode::Char('A'), _) => Some(Action::MsPageCycleFilter),
        (KeyCode::Char('S'), _) => Some(Action::MsPageCycleSort),
        (KeyCode::Char('E'), _) => Some(Action::MsPageEdit),
        (KeyCode::Char('N'), _) => Some(Action::MsPageNew),
        (KeyCode::Char('C'), _) => Some(Action::MsPageCycleStatus),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => Some(Action::MsPageInput(c)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ke(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn ctrl_c_quits_immediately_from_any_mode() {
        use crate::app::{MilestonePageState, Mode, PickerState};
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let mut app = App::new();
        assert!(matches!(map_key(ctrl_c, &mut app), Some(Action::Quit)));
        app.mode = Mode::ProjectPicker(PickerState {
            query: String::new(),
            items: vec![],
            cursor: 0,
        });
        assert!(matches!(map_key(ctrl_c, &mut app), Some(Action::Quit)));
        app.mode = Mode::MilestonePage(MilestonePageState::default());
        assert!(matches!(map_key(ctrl_c, &mut app), Some(Action::Quit)));
        // A plain `c` still types into the picker/page query.
        assert!(matches!(
            map_key(ke(KeyCode::Char('c')), &mut app),
            Some(Action::MsPageInput('c'))
        ));
    }

    #[test]
    fn confirm_quit_accepts_enter_and_a_second_q() {
        use crate::app::Mode;
        let mut app = App::new();
        app.mode = Mode::ConfirmQuit;
        for code in [KeyCode::Enter, KeyCode::Char('q'), KeyCode::Char('y')] {
            assert!(
                matches!(map_key(ke(code), &mut app), Some(Action::Quit)),
                "{code:?} should confirm"
            );
        }
        assert!(matches!(
            map_key(ke(KeyCode::Char('n')), &mut app),
            Some(Action::Cancel)
        ));
    }

    #[test]
    fn detail_view_edits_in_place_and_closes_on_the_usual_keys() {
        use crate::app::Mode;
        let mut app = App::new();
        app.mode = Mode::Detail("PULSE-1".into());
        assert!(matches!(
            map_key(ke(KeyCode::Char('e')), &mut app),
            Some(Action::EditCard)
        ));
        for code in [KeyCode::Esc, KeyCode::Char('q'), KeyCode::Enter] {
            assert!(
                matches!(map_key(ke(code), &mut app), Some(Action::Cancel)),
                "{code:?} should close the detail view"
            );
        }
    }

    #[test]
    fn space_begins_move_then_letter_resolves_status() {
        let mut app = App::new();
        assert!(matches!(
            map_key(ke(KeyCode::Char(' ')), &mut app),
            Some(Action::BeginMove)
        ));
        app.mode = Mode::AwaitingMove;
        match map_key(ke(KeyCode::Char('d')), &mut app) {
            Some(Action::MoveTo(s)) => assert_eq!(s, "done"),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn milestone_page_types_into_the_filter_but_reserves_capitals() {
        use crate::app::{MilestonePageState, Mode};
        let mut app = App::new();
        app.mode = Mode::MilestonePage(MilestonePageState::default());
        let cap = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);

        // Lowercase types into the query...
        assert!(matches!(
            map_key(ke(KeyCode::Char('a')), &mut app),
            Some(Action::MsPageInput('a'))
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('s')), &mut app),
            Some(Action::MsPageInput('s'))
        ));
        // ...except j/k, which navigate like everywhere else.
        assert!(matches!(
            map_key(ke(KeyCode::Char('j')), &mut app),
            Some(Action::MsPageDown)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('k')), &mut app),
            Some(Action::MsPageUp)
        ));
        // Capitals are the page's commands.
        assert!(matches!(
            map_key(cap('E'), &mut app),
            Some(Action::MsPageEdit)
        ));
        assert!(matches!(
            map_key(cap('N'), &mut app),
            Some(Action::MsPageNew)
        ));
        assert!(matches!(
            map_key(cap('S'), &mut app),
            Some(Action::MsPageCycleSort)
        ));
        assert!(matches!(
            map_key(cap('C'), &mut app),
            Some(Action::MsPageCycleStatus)
        ));
        // Tab and A both cycle the status bucket.
        assert!(matches!(
            map_key(cap('A'), &mut app),
            Some(Action::MsPageCycleFilter)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Tab), &mut app),
            Some(Action::MsPageCycleFilter)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Backspace), &mut app),
            Some(Action::MsPageBackspace)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Enter), &mut app),
            Some(Action::MsPageSelect)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Esc), &mut app),
            Some(Action::Cancel)
        ));
    }

    #[test]
    fn normal_keys_dispatch_to_cliban_actions() {
        let mut app = App::new();
        assert!(matches!(
            map_key(ke(KeyCode::Char('n')), &mut app),
            Some(Action::NewIssue)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('N')), &mut app),
            Some(Action::NewMilestone)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('t')), &mut app),
            Some(Action::TagMilestone)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('a')), &mut app),
            Some(Action::Archive)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('m')), &mut app),
            Some(Action::OpenMilestonePage)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('M')), &mut app),
            Some(Action::CycleMilestoneFilter)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('e')), &mut app),
            Some(Action::EditCard)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('E')), &mut app),
            Some(Action::EditScope)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('p')), &mut app),
            Some(Action::OpenProjectPicker)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('/')), &mut app),
            Some(Action::OpenFuzzyFind)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('r')), &mut app),
            Some(Action::Refresh)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('q')), &mut app),
            Some(Action::QuitRequest)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Enter), &mut app),
            Some(Action::OpenDetail)
        ));
    }
}
