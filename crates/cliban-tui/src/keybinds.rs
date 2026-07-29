//! crossterm KeyEvent → Action, dispatched on Mode. Pure.
use crate::actions::{Action, Direction};
use crate::app::{App, Mode};
use crate::session::MouseInput;
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
            // Chase the dependency chain: land on the first open blocker.
            KeyCode::Char('b') => Some(Action::JumpToBlocker),
            _ => None,
        },
        Mode::ConfirmQuit => map_confirm_quit(key),
        Mode::ConfirmArchive(_) => map_confirm_archive(key),
        Mode::ConfirmProjectArchive { .. } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                Some(Action::ProjPageArchive)
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Some(Action::Cancel),
            _ => None,
        },
        Mode::NewDialog(_) => map_new_dialog(key),
        Mode::MilestonePicker(_) => map_picker(key),
        Mode::FuzzyFind(_) => map_fuzzy(key),
        Mode::MilestonePage(_) => map_milestone_page(key),
        Mode::ProjectPage(_) => map_project_page(key),
        Mode::ActivityPage(_) => map_activity_page(key),
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
        // Direct column jumps, lazydocker-style.
        (KeyCode::Char(c @ '1'..='5'), KeyModifiers::NONE) => {
            Some(Action::FocusColumn(c as usize - '1' as usize))
        }
        (KeyCode::Char('d'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
            Some(Action::PageMove(Direction::Down))
        }
        (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
            Some(Action::PageMove(Direction::Up))
        }
        (KeyCode::Char('u'), KeyModifiers::NONE) => Some(Action::Undo),
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
        // The mailbox gets the lowercase key: checking activity is routine,
        // archiving is not.
        (KeyCode::Char('a'), KeyModifiers::NONE) => Some(Action::OpenActivityPage),
        (KeyCode::Char('A'), _) => Some(Action::ArchiveRequest),
        (KeyCode::Char('m'), KeyModifiers::NONE) => Some(Action::OpenMilestonePage),
        (KeyCode::Char('M'), _) => Some(Action::CycleMilestoneFilter),
        (KeyCode::Char('p'), KeyModifiers::NONE) => Some(Action::OpenProjectPage),
        (KeyCode::Char('/'), _) => Some(Action::OpenFuzzyFind),
        (KeyCode::Char('r'), KeyModifiers::NONE) => Some(Action::Refresh),
        (KeyCode::Char('?'), _) => Some(Action::ToggleHelp),
        // Esc means "back", and the board is already the top level — quitting
        // belongs to q/Ctrl-C, not to someone spamming Esc out of popups.
        _ => None,
    }
}

/// Mouse gestures → actions, resolved against the last-drawn frame's hit
/// map. Clicks only act where they're unambiguous: a card (click to focus,
/// click the focused card to open it), a column's empty space, or a page
/// row. Anything under a dialog is unreachable — dialogs register no
/// regions. The wheel moves whatever cursor the mode owns.
pub fn map_mouse(m: MouseInput, app: &App, hits: &crate::ui::HitMap) -> Option<Action> {
    use crate::ui::Hit;
    match m {
        MouseInput::ScrollUp(..) | MouseInput::ScrollDown(..) => {
            let down = matches!(m, MouseInput::ScrollDown(..));
            match &app.mode {
                Mode::Normal => Some(Action::FocusMove(if down {
                    Direction::Down
                } else {
                    Direction::Up
                })),
                Mode::MilestonePage(_) => Some(if down {
                    Action::MsPageDown
                } else {
                    Action::MsPageUp
                }),
                Mode::ProjectPage(_) => Some(if down {
                    Action::ProjPageDown
                } else {
                    Action::ProjPageUp
                }),
                Mode::ActivityPage(_) => Some(if down {
                    Action::ActPageDown
                } else {
                    Action::ActPageUp
                }),
                _ => None,
            }
        }
        MouseInput::Down(x, y) => match hits.at(x, y)? {
            Hit::Card { column, idx } => match &app.mode {
                Mode::Normal => {
                    if app.focus.column == *column && app.focus.card_idx == *idx {
                        Some(Action::OpenDetail)
                    } else {
                        Some(Action::FocusCard(*column, *idx))
                    }
                }
                _ => None,
            },
            Hit::Column(col) => match &app.mode {
                Mode::Normal => Some(Action::FocusCard(*col, usize::MAX)),
                _ => None,
            },
            Hit::PageRow(i) => Some(Action::PageCursor(*i)),
        },
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

fn map_confirm_archive(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => Some(Action::Archive),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Some(Action::Cancel),
        _ => None,
    }
}

// Filter boxes follow the fzf/readline convention: every printable character
// types into the query — including j/k, so a project named "jira" is
// reachable — and navigation is arrows or Ctrl-n/p. (Ctrl-j is off the
// table: it arrives as a bare LF byte, indistinguishable from Enter.)

/// The quick-create form: every printable character types into the focused
/// field, Tab/arrows move between fields, Enter creates, Esc cancels.
fn map_new_dialog(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::NewDialogConfirm),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::NewDialogBackspace),
        (KeyCode::Tab, _) | (KeyCode::Down, _) => Some(Action::NewDialogNextField),
        (KeyCode::BackTab, _) | (KeyCode::Up, _) => Some(Action::NewDialogPrevField),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
            Some(Action::NewDialogInput(c))
        }
        _ => None,
    }
}

fn map_picker(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::PickerConfirm),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::PickerBackspace),
        (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => Some(Action::PickerUp),
        (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            Some(Action::PickerDown)
        }
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => Some(Action::PickerInput(c)),
        _ => None,
    }
}

fn map_fuzzy(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::FuzzyConfirm),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::FuzzyBackspace),
        (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => Some(Action::FuzzyUp),
        (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => Some(Action::FuzzyDown),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => Some(Action::FuzzyInput(c)),
        _ => None,
    }
}

fn map_milestone_page(key: KeyEvent) -> Option<Action> {
    // Typing filters, as on the project picker — the whole lowercase alphabet
    // (j/k included) goes to the query, and every page *command* is a capital
    // letter or a named key. Navigation is arrows or Ctrl-n/p, plus
    // G/Home/End jumps and PgUp/PgDn half-pages for long lists. The filter is
    // case-insensitive, so lowercase typing matches capitalized names.
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::MsPageSelect),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::MsPageBackspace),
        (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            Some(Action::MsPageDown)
        }
        (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => Some(Action::MsPageUp),
        (KeyCode::PageDown, _) => Some(Action::MsPagePage(Direction::Down)),
        (KeyCode::PageUp, _) => Some(Action::MsPagePage(Direction::Up)),
        (KeyCode::Home, _) => Some(Action::MsPageTop),
        (KeyCode::Char('G'), _) | (KeyCode::End, _) => Some(Action::MsPageBottom),
        (KeyCode::Tab, _) => Some(Action::MsPageCycleFilter),
        (KeyCode::BackTab, _) => Some(Action::MsPageCycleFilterBack),
        (KeyCode::Char('S'), _) => Some(Action::MsPageCycleSort),
        (KeyCode::Char('E'), _) => Some(Action::MsPageEdit),
        (KeyCode::Char('N'), _) => Some(Action::MsPageNew),
        (KeyCode::Char('C'), _) => Some(Action::MsPageCycleStatus),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => Some(Action::MsPageInput(c)),
        _ => None,
    }
}

/// Same grammar as the milestone page: lowercase types into the filter,
/// capitals are commands, Ctrl-n/p and named keys navigate. `A` here means
/// archive (the page's headline feature), not the Tab alias.
fn map_project_page(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::ProjPageSelect),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::ProjPageBackspace),
        (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            Some(Action::ProjPageDown)
        }
        (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => Some(Action::ProjPageUp),
        (KeyCode::PageDown, _) => Some(Action::ProjPagePage(Direction::Down)),
        (KeyCode::PageUp, _) => Some(Action::ProjPagePage(Direction::Up)),
        (KeyCode::Home, _) => Some(Action::ProjPageTop),
        (KeyCode::Char('G'), _) | (KeyCode::End, _) => Some(Action::ProjPageBottom),
        (KeyCode::Tab, _) => Some(Action::ProjPageCycleFilter),
        (KeyCode::BackTab, _) => Some(Action::ProjPageCycleFilterBack),
        (KeyCode::Char('S'), _) => Some(Action::ProjPageCycleSort),
        (KeyCode::Char('E'), _) => Some(Action::ProjPageEdit),
        (KeyCode::Char('N'), _) => Some(Action::ProjPageNew),
        (KeyCode::Char('A'), _) => Some(Action::ProjPageArchiveRequest),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
            Some(Action::ProjPageInput(c))
        }
        _ => None,
    }
}

/// The mailbox: same filter-box grammar, no edit/new/sort — just move,
/// filter by kind (Tab), and Enter to jump to the issue on the board.
fn map_activity_page(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::ActPageSelect),
        (KeyCode::Esc, _) => Some(Action::Cancel),
        (KeyCode::Backspace, _) => Some(Action::ActPageBackspace),
        (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            Some(Action::ActPageDown)
        }
        (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => Some(Action::ActPageUp),
        (KeyCode::PageDown, _) => Some(Action::ActPagePage(Direction::Down)),
        (KeyCode::PageUp, _) => Some(Action::ActPagePage(Direction::Up)),
        (KeyCode::Home, _) => Some(Action::ActPageTop),
        (KeyCode::Char('G'), _) | (KeyCode::End, _) => Some(Action::ActPageBottom),
        (KeyCode::Tab, _) => Some(Action::ActPageCycleFilter),
        (KeyCode::BackTab, _) => Some(Action::ActPageCycleFilterBack),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
            Some(Action::ActPageInput(c))
        }
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
    fn a_opens_the_activity_page_where_typing_filters_and_tab_cycles() {
        use crate::app::{ActivityPageState, Mode};
        let mut app = App::new();
        assert!(matches!(
            map_key(ke(KeyCode::Char('a')), &mut app),
            Some(Action::OpenActivityPage)
        ));
        app.mode = Mode::ActivityPage(ActivityPageState::default());
        assert!(matches!(
            map_key(ke(KeyCode::Char('j')), &mut app),
            Some(Action::ActPageInput('j'))
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Tab), &mut app),
            Some(Action::ActPageCycleFilter)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::BackTab), &mut app),
            Some(Action::ActPageCycleFilterBack)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Enter), &mut app),
            Some(Action::ActPageSelect)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Esc), &mut app),
            Some(Action::Cancel)
        ));
    }

    #[test]
    fn esc_is_not_a_quit_key_on_the_board() {
        let mut app = App::new();
        assert!(
            map_key(ke(KeyCode::Esc), &mut app).is_none(),
            "Esc means back, and the board is already the top level"
        );
    }

    #[test]
    fn filter_boxes_accept_j_and_k_as_text_and_navigate_with_ctrl_n_p() {
        use crate::app::{FuzzyState, Mode, PickerState};
        let ctrl = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        let mut app = App::new();
        app.mode = Mode::MilestonePicker(PickerState {
            query: String::new(),
            items: vec![],
            cursor: 0,
        });
        assert!(matches!(
            map_key(ke(KeyCode::Char('j')), &mut app),
            Some(Action::PickerInput('j'))
        ));
        assert!(matches!(
            map_key(ctrl('n'), &mut app),
            Some(Action::PickerDown)
        ));
        assert!(matches!(
            map_key(ctrl('p'), &mut app),
            Some(Action::PickerUp)
        ));

        app.mode = Mode::FuzzyFind(FuzzyState {
            query: String::new(),
            results: vec![],
            cursor: 0,
        });
        assert!(matches!(
            map_key(ke(KeyCode::Char('k')), &mut app),
            Some(Action::FuzzyInput('k'))
        ));
        assert!(matches!(
            map_key(ctrl('n'), &mut app),
            Some(Action::FuzzyDown)
        ));
    }

    #[test]
    fn number_keys_and_half_pages_navigate_the_board() {
        let mut app = App::new();
        assert!(matches!(
            map_key(ke(KeyCode::Char('3')), &mut app),
            Some(Action::FocusColumn(2))
        ));
        let ctrl = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        assert!(matches!(
            map_key(ctrl('d'), &mut app),
            Some(Action::PageMove(Direction::Down))
        ));
        assert!(matches!(
            map_key(ke(KeyCode::PageUp), &mut app),
            Some(Action::PageMove(Direction::Up))
        ));
    }

    #[test]
    fn archive_asks_first_and_the_dialog_answers_like_quit() {
        use crate::app::Mode;
        let mut app = App::new();
        app.mode = Mode::ConfirmArchive("PULSE-9".into());
        for code in [KeyCode::Char('y'), KeyCode::Enter] {
            assert!(
                matches!(map_key(ke(code), &mut app), Some(Action::Archive)),
                "{code:?} should confirm the archive"
            );
        }
        for code in [KeyCode::Char('n'), KeyCode::Esc] {
            assert!(
                matches!(map_key(ke(code), &mut app), Some(Action::Cancel)),
                "{code:?} should decline the archive"
            );
        }
    }

    #[test]
    fn ctrl_c_quits_immediately_from_any_mode() {
        use crate::app::{MilestonePageState, Mode, PickerState};
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let mut app = App::new();
        assert!(matches!(map_key(ctrl_c, &mut app), Some(Action::Quit)));
        app.mode = Mode::MilestonePicker(PickerState {
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
        // ...including j and k, so names containing them stay searchable.
        // Navigation is Ctrl-n/p (or arrows), fzf-style.
        assert!(matches!(
            map_key(ke(KeyCode::Char('j')), &mut app),
            Some(Action::MsPageInput('j'))
        ));
        assert!(matches!(
            map_key(ke(KeyCode::Char('k')), &mut app),
            Some(Action::MsPageInput('k'))
        ));
        let ctrl = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        assert!(matches!(
            map_key(ctrl('n'), &mut app),
            Some(Action::MsPageDown)
        ));
        assert!(matches!(
            map_key(ctrl('p'), &mut app),
            Some(Action::MsPageUp)
        ));
        assert!(matches!(
            map_key(cap('G'), &mut app),
            Some(Action::MsPageBottom)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::BackTab), &mut app),
            Some(Action::MsPageCycleFilterBack)
        ));
        assert!(matches!(
            map_key(ke(KeyCode::PageDown), &mut app),
            Some(Action::MsPagePage(Direction::Down))
        ));
        // Capitals are the page's commands. (`A` is reserved across both
        // pages for the project page's archive — no Tab alias here.)
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
        // Tab cycles the status bucket; `A` types (it is the project page's
        // archive key, kept out of this page's command set on purpose).
        assert!(matches!(
            map_key(ke(KeyCode::Tab), &mut app),
            Some(Action::MsPageCycleFilter)
        ));
        assert!(matches!(
            map_key(cap('A'), &mut app),
            Some(Action::MsPageInput('A'))
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
            Some(Action::OpenActivityPage)
        ));
        assert!(matches!(
            map_key(
                KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT),
                &mut app
            ),
            Some(Action::ArchiveRequest)
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
            Some(Action::OpenProjectPage)
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
