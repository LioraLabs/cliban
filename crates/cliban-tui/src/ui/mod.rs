//! Ratatui rendering for the cliban TUI kanban board.

pub mod activity_page;
pub mod board;
pub mod card;
pub mod confirm_quit;
pub mod detail;
pub mod fuzzy;
pub mod help;
pub mod milestone_page;
pub mod picker;
pub mod project_page;
pub mod theme;
pub mod top_bar;

use crate::app::{App, Mode};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

// The footer favors the everyday keys and the two discovery anchors
// (`p` scope, `?` help); the full map — E, M, gg/G, H/J/K/L — lives in help.
const STATUS_HELP: &[(&str, &str)] = &[
    ("hjkl", "move"),
    ("enter", "detail"),
    ("e", "edit"),
    ("n", "new"),
    ("Space", "mv"),
    ("a", "arch"),
    ("A", "activity"),
    ("p", "project"),
    ("m", "milestones"),
    ("/", "find"),
    ("r", "refresh"),
    ("?", "help"),
    ("q", "quit"),
];

/// The `Space` move menu, shown in the footer while the app waits for a
/// status letter — otherwise the pending mode is invisible.
const MOVE_HELP: &[(&str, &str)] = &[
    ("b", "backlog"),
    ("i", "in-progress"),
    ("k", "blocked"),
    ("r", "in-review"),
    ("d", "done"),
];

pub fn render(frame: &mut Frame, app: &App) {
    // The milestone and project pages own the whole screen — pages, not popups.
    if let Mode::MilestonePage(state) = &app.mode {
        milestone_page::draw(frame, frame.area(), app, state);
        return;
    }
    if let Mode::ProjectPage(state) = &app.mode {
        project_page::draw(frame, frame.area(), app, state);
        return;
    }
    if let Mode::ActivityPage(state) = &app.mode {
        activity_page::draw(frame, frame.area(), app, state);
        return;
    }
    // The archive dialog layers over the page it interrupted.
    if let Mode::ConfirmProjectArchive {
        key,
        archived,
        page,
    } = &app.mode
    {
        project_page::draw(frame, frame.area(), app, page);
        confirm_quit::draw_confirm_project(frame, frame.area(), key, *archived);
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());
    top_bar::draw(frame, chunks[0], app);
    board::draw_board(frame, chunks[1], app);
    // Status messages are feedback — they get the marker color and push the
    // hints right; the hints alone otherwise fill the row quietly. While a
    // Space-move is pending the footer becomes that menu instead.
    let mut status = if matches!(app.mode, Mode::AwaitingMove) {
        let mut line = theme::hints(MOVE_HELP);
        let mut spans = vec![Span::styled(
            "move to:  ",
            Style::default().fg(theme::MARKER),
        )];
        spans.append(&mut line.spans);
        spans.push(Span::styled(
            "   any other key cancels",
            Style::default().fg(theme::DIM),
        ));
        ratatui::text::Line::from(spans)
    } else {
        theme::hints(STATUS_HELP)
    };
    if let Some(m) = &app.status_msg {
        let mut spans = vec![
            Span::styled(m.clone(), Style::default().fg(theme::MARKER)),
            Span::styled("  |  ", Style::default().fg(theme::DIM)),
        ];
        spans.extend(status.spans);
        status = ratatui::text::Line::from(spans);
    }
    frame.render_widget(Paragraph::new(status), chunks[2]);

    match &app.mode {
        Mode::Help => help::draw_help(frame, frame.area()),
        Mode::ConfirmQuit => confirm_quit::draw_confirm_quit(frame, frame.area()),
        Mode::ConfirmArchive(key) => confirm_quit::draw_confirm_archive(frame, frame.area(), key),
        Mode::Detail(key) => {
            if let Some(c) = app.cards.iter().find(|c| &c.key == key) {
                detail::draw(frame, frame.area(), c);
            }
        }
        Mode::MilestonePicker(p) => {
            let labels: Vec<String> = p.items.iter().map(|c| c.label.clone()).collect();
            let idx = picker::fuzzy_indices(&labels, &p.query);
            let filtered: Vec<String> = idx.iter().map(|&i| labels[i].clone()).collect();
            picker::draw(
                frame,
                frame.area(),
                picker::PickerView {
                    title: "Pick milestone",
                    query: &p.query,
                    items: &filtered,
                    cursor: p.cursor,
                },
            );
        }
        Mode::FuzzyFind(state) => fuzzy::draw(frame, frame.area(), app, state),
        // Handled above — they replace the board rather than layering over it.
        Mode::MilestonePage(_)
        | Mode::ProjectPage(_)
        | Mode::ActivityPage(_)
        | Mode::ConfirmProjectArchive { .. } => {}
        // AwaitingMove keeps the board visible; its menu lives in the footer.
        Mode::Normal | Mode::AwaitingMove => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn dump(app: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(160, 24)).unwrap();
        t.draw(|f| render(f, app)).unwrap();
        let buf = t.backend().buffer().clone();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn footer_shows_discovery_anchors() {
        let d = dump(&App::new());
        for needle in ["p project", "? help", "q quit"] {
            assert!(d.contains(needle), "footer missing `{needle}`:\n{d}");
        }
    }

    #[test]
    fn awaiting_move_turns_the_footer_into_the_status_menu() {
        let mut app = App::new();
        app.mode = Mode::AwaitingMove;
        let d = dump(&app);
        assert!(d.contains("move to:"), "pending move must be visible:\n{d}");
        for needle in ["b backlog", "i in-progress", "d done", "cancels"] {
            assert!(d.contains(needle), "move menu missing `{needle}`:\n{d}");
        }
        // The regular hints are replaced, not appended.
        assert!(!d.contains("m milestones"), "{d}");
    }
}
