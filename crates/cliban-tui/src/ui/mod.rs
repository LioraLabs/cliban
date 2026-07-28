//! Ratatui rendering for the cliban TUI kanban board.

pub mod board;
pub mod card;
pub mod confirm_quit;
pub mod detail;
pub mod fuzzy;
pub mod help;
pub mod milestone_page;
pub mod picker;
pub mod theme;
pub mod top_bar;

use crate::app::{App, Mode};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const STATUS_HELP: &[(&str, &str)] = &[
    ("hjkl", "move"),
    ("enter", "detail"),
    ("e", "edit"),
    ("E", "proj/ms"),
    ("n", "new"),
    ("N", "ms+"),
    ("t", "tag"),
    ("Space", "mv"),
    ("a", "arch"),
    ("m", "milestones"),
    ("M", "filter"),
    ("/", "find"),
    ("r", "refresh"),
    ("q", "quit"),
];

pub fn render(frame: &mut Frame, app: &App) {
    // The milestone page owns the whole screen — it's a page, not a popup.
    if let Mode::MilestonePage(state) = &app.mode {
        milestone_page::draw(frame, frame.area(), app, state);
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
    // hints right; the hints alone otherwise fill the row quietly.
    let mut status = theme::hints(STATUS_HELP);
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
        Mode::Detail(key) => {
            if let Some(c) = app.cards.iter().find(|c| &c.key == key) {
                detail::draw(frame, frame.area(), c);
            }
        }
        Mode::ProjectPicker(p) | Mode::MilestonePicker(p) => {
            let labels: Vec<String> = p.items.iter().map(|c| c.label.clone()).collect();
            let idx = picker::fuzzy_indices(&labels, &p.query);
            let filtered: Vec<String> = idx.iter().map(|&i| labels[i].clone()).collect();
            let title = if matches!(app.mode, Mode::ProjectPicker(_)) {
                "Pick project"
            } else {
                "Pick milestone"
            };
            picker::draw(
                frame,
                frame.area(),
                picker::PickerView {
                    title,
                    query: &p.query,
                    items: &filtered,
                    cursor: p.cursor,
                },
            );
        }
        Mode::FuzzyFind(state) => fuzzy::draw(frame, frame.area(), app, state),
        // Handled above — it replaces the board rather than layering over it.
        Mode::MilestonePage(_) => {}
        Mode::Normal | Mode::AwaitingMove => {}
    }
}
