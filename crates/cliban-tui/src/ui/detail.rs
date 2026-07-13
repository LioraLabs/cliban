use crate::app::Card;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, area: Rect, card: &Card) {
    let popup = centered_rect(70, 20, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(format!(" {} ", card.key))
        .borders(Borders::ALL);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut lines = vec![
        Line::from(card.title.clone()),
        Line::styled(
            format!("status: {}   priority: {}", card.status, card.priority),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    if let Some(m) = &card.milestone {
        lines.push(Line::styled(
            format!("milestone: {m}"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "q/esc back",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
fn centered_rect(width_pct: u16, height_max: u16, area: Rect) -> Rect {
    let w = (area.width * width_pct / 100).max(20).min(area.width);
    let h = height_max.min(area.height);
    Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    #[test]
    fn detail_renders_key_and_title() {
        let c = Card {
            id: 0,
            key: "CLI-8".into(),
            project: "CLI".into(),
            title: "Build TUI".into(),
            status: "backlog".into(),
            priority: "high".into(),
            position: 1.0,
            milestone_id: None,
            milestone: None,
        };
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| draw(f, Rect::new(0, 0, 80, 24), &c)).unwrap();
        let buf = t.backend().buffer();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        assert!(s.contains("CLI-8"));
        assert!(s.contains("Build TUI"));
    }
}
