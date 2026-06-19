//! Ratatui rendering for the cliban TUI kanban board.

pub mod board; pub mod card; pub mod confirm_quit; pub mod detail; pub mod fuzzy;
pub mod help; pub mod milestone_overlay; pub mod picker; pub mod top_bar;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use crate::app::{App, Mode};

const STATUS_HELP: &str = "hjkl move  enter detail  e edit  E proj/ms  n new  N ms+  t tag  Space mv  a arch  m ms  M filter  / find  r refresh  q quit";

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).split(frame.area());
    top_bar::draw(frame, chunks[0], app);
    board::draw_board(frame, chunks[1], app);
    let status = match &app.status_msg { Some(m) => format!("{m}  |  {STATUS_HELP}"), None => STATUS_HELP.to_string() };
    frame.render_widget(Paragraph::new(Line::styled(status, Style::default().fg(Color::Gray))), chunks[2]);

    match &app.mode {
        Mode::Help => help::draw_help(frame, frame.area()),
        Mode::ConfirmQuit => confirm_quit::draw_confirm_quit(frame, frame.area()),
        Mode::Detail(key) => { if let Some(c) = app.cards.iter().find(|c| &c.key == key) { detail::draw(frame, frame.area(), c); } }
        Mode::ProjectPicker(p) | Mode::MilestonePicker(p) => {
            let labels: Vec<String> = p.items.iter().map(|c| c.label.clone()).collect();
            let idx = picker::fuzzy_indices(&labels, &p.query);
            let filtered: Vec<String> = idx.iter().map(|&i| labels[i].clone()).collect();
            let title = if matches!(app.mode, Mode::ProjectPicker(_)) { "Pick project" } else { "Pick milestone" };
            picker::draw(frame, frame.area(), picker::PickerView { title, query: &p.query, items: &filtered, cursor: p.cursor });
        }
        Mode::FuzzyFind(state) => fuzzy::draw(frame, frame.area(), app, state),
        Mode::MilestoneOverlay(state) => milestone_overlay::draw(frame, frame.area(), state),
        Mode::Normal | Mode::AwaitingMove => {}
    }
}
