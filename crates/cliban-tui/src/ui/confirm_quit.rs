use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn draw_confirm_quit(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(40, 5, area);
    frame.render_widget(Clear, popup);
    let block = Block::default().title(" Quit ").borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    let body = vec![Line::raw(""), Line::from(Span::raw("  Quit cliban?  [y]es / [n]o")), Line::raw("")];
    frame.render_widget(Paragraph::new(body).block(block), popup);
}
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let h = height.min(area.height); let w = width.min(area.width);
    Rect::new(area.x + (area.width.saturating_sub(w))/2, area.y + (area.height.saturating_sub(h))/2, w, h)
}
