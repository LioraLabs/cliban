//! Minimal blocking list picker: title + items, j/k/arrows to move, Enter
//! selects, Esc/q/Ctrl-C declines. Runs over any Backend + Session — used
//! as the SSH tenant picker, headless-testable like the board.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListState};
use ratatui::{Frame, Terminal};

use crate::session::{Session, SessionEvent};

type DynErr = Box<dyn std::error::Error>;

/// Block until the user picks an item (`Some(index)`) or declines (`None`).
/// An empty `items` list declines immediately without reading any input.
pub fn pick<B: Backend>(
    terminal: &mut Terminal<B>,
    session: &mut dyn Session,
    title: &str,
    items: &[String],
) -> Result<Option<usize>, DynErr> {
    if items.is_empty() {
        return Ok(None);
    }
    let mut cursor = 0usize;
    loop {
        terminal.draw(|f| render(f, title, items, cursor))?;
        let key = match session.next_event(Duration::from_millis(100))? {
            SessionEvent::Tick | SessionEvent::Resize(..) | SessionEvent::Refresh => continue,
            SessionEvent::Key(k) => k,
        };
        match action(key) {
            Some(Move::Up) => cursor = cursor.saturating_sub(1),
            Some(Move::Down) => cursor = (cursor + 1).min(items.len().saturating_sub(1)),
            Some(Move::Accept) => return Ok(Some(cursor)),
            Some(Move::Cancel) => return Ok(None),
            None => {}
        }
    }
}

enum Move {
    Up,
    Down,
    Accept,
    Cancel,
}

fn action(key: KeyEvent) -> Option<Move> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => Some(Move::Cancel),
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => Some(Move::Up),
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => Some(Move::Down),
        (KeyCode::Enter, _) => Some(Move::Accept),
        (KeyCode::Esc, _) | (KeyCode::Char('q'), _) => Some(Move::Cancel),
        _ => None,
    }
}

fn render(f: &mut Frame, title: &str, items: &[String], cursor: usize) {
    let longest = items
        .iter()
        .map(String::len)
        .chain([title.len()])
        .max()
        .unwrap_or(0) as u16;
    let width = (longest + 6).min(f.area().width);
    let height = (items.len() as u16 + 2).min(f.area().height);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(f.area());
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let list = List::new(items.iter().map(String::as_str))
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = ListState::default().with_selected(Some(cursor));
    f.render_stateful_widget(list, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ByteSession;
    use ratatui::backend::TestBackend;

    fn items() -> Vec<String> {
        vec!["team-a".into(), "team-b".into(), "team-c".into()]
    }

    fn run(bytes: &[u8]) -> (Option<usize>, String) {
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
        let mut session = ByteSession::new();
        session.feed_bytes(bytes);
        let picked = pick(&mut terminal, &mut session, "pick a board", &items()).unwrap();
        let buf = terminal.backend().buffer();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        (picked, s)
    }

    #[test]
    fn renders_title_and_items() {
        let (_, frame) = run(b"\r");
        assert!(frame.contains("pick a board"), "{frame}");
        assert!(frame.contains("team-a"), "{frame}");
        assert!(frame.contains("team-c"), "{frame}");
    }

    #[test]
    fn enter_picks_first_by_default() {
        assert_eq!(run(b"\r").0, Some(0));
    }

    #[test]
    fn j_and_arrow_down_move_then_enter_picks() {
        assert_eq!(run(b"j\r").0, Some(1));
        assert_eq!(run(b"j\x1b[B\r").0, Some(2));
        // Down saturates at the last item.
        assert_eq!(run(b"jjjjj\r").0, Some(2));
        // k moves back up and saturates at the first.
        assert_eq!(run(b"jjkkk\r").0, Some(0));
    }

    #[test]
    fn q_esc_and_ctrl_c_cancel() {
        assert_eq!(run(b"q").0, None);
        assert_eq!(run(&[0x03]).0, None);
        // Lone ESC arrives via the idle flush path.
        assert_eq!(run(b"\x1b").0, None);
    }

    #[test]
    fn empty_items_declines_immediately_without_reading_input() {
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
        // No bytes fed at all, so if `pick` fell through to the read loop it
        // would spin on Ticks forever instead of returning; getting `None`
        // back proves the empty-items guard short-circuits before that.
        let mut session = ByteSession::new();
        let picked = pick(&mut terminal, &mut session, "pick a board", &[]).unwrap();
        assert_eq!(picked, None);
    }
}
