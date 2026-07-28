use super::theme;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

pub fn draw_confirm_quit(frame: &mut Frame, area: Rect) {
    draw_confirm(frame, area, "Quit", "Quit cliban?");
}

pub fn draw_confirm_archive(frame: &mut Frame, area: Rect, key: &str) {
    draw_confirm(frame, area, "Archive", &format!("Archive {key}?"));
}

/// A small yes/no dialog; `y`/Enter confirm, `n`/Esc decline.
fn draw_confirm(frame: &mut Frame, area: Rect, title: &str, question: &str) {
    let width = (question.chars().count() as u16 + 18).max(40);
    let popup = centered_rect(width, 5, area);
    frame.render_widget(Clear, popup);
    let block = theme::popup_block(title, theme::MARKER);
    let body = vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw(format!("  {question}  ")),
            Span::styled("[y]es", Style::default().fg(Color::Indexed(78))),
            Span::raw(" / "),
            Span::styled("[n]o", Style::default().fg(Color::Indexed(203))),
        ]),
        Line::raw(""),
    ];
    frame.render_widget(Paragraph::new(body).block(block), popup);
}
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let h = height.min(area.height);
    let w = width.min(area.width);
    Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    )
}
