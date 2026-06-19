use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use crate::app::MilestoneOverlayState;

pub fn draw(frame: &mut Frame, area: Rect, state: &MilestoneOverlayState) {
    let popup = centered_rect(60, 20, area);
    frame.render_widget(Clear, popup);
    let block = Block::default().title(" Milestones ").borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut lines: Vec<Line> = Vec::new();
    if state.items.is_empty() {
        lines.push(Line::styled("  (no milestones)", Style::default().fg(Color::DarkGray)));
    } else {
        for (i, m) in state.items.iter().enumerate() {
            let target = m.target.clone().unwrap_or_else(|| "-".into());
            let text = format!("{:<18} {:<10} {}", m.name, m.status, target);
            if i == state.cursor {
                lines.push(Line::from(vec![Span::styled("▸ ", Style::default().fg(Color::Yellow)),
                    Span::styled(text, Style::default().add_modifier(Modifier::REVERSED))]));
            } else { lines.push(Line::from(vec![Span::raw("  "), Span::raw(text)])); }
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("  j/k move   E edit   esc close", Style::default().fg(Color::DarkGray)));
    frame.render_widget(Paragraph::new(lines), inner);
}
fn centered_rect(width_pct: u16, height_max: u16, area: Rect) -> Rect {
    let w = (area.width * width_pct / 100).max(20).min(area.width);
    let h = height_max.min(area.height);
    Rect::new(area.x + (area.width.saturating_sub(w))/2, area.y + (area.height.saturating_sub(h))/2, w, h)
}
