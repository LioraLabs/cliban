use super::theme;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

pub fn draw_confirm_quit(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(40, 5, area);
    frame.render_widget(Clear, popup);
    let block = theme::popup_block("Quit", theme::MARKER);
    let body = vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw("  Quit cliban?  "),
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
