use super::card::priority_letter;
use super::theme;
use crate::app::Card;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, area: Rect, card: &Card) {
    let popup = centered_rect(70, 20, area);
    frame.render_widget(Clear, popup);
    let block = theme::popup_block(&card.key, theme::ACCENT);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // A label column in dim gray, values in the same colors the board uses:
    // status wears its column hue, priority its ramp, milestone its violet.
    let dim = Style::default().fg(theme::DIM);
    let field = |label: &str| Span::styled(format!("  {label:<11}"), dim);
    let mut lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            format!("  {}", card.title),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(vec![
            field("status"),
            Span::styled(
                card.status.clone(),
                Style::default().fg(theme::column_color(&card.status)),
            ),
        ]),
        Line::from(vec![
            field("priority"),
            Span::styled(
                format!("{} {}", card.priority, priority_letter(&card.priority)),
                Style::default().fg(theme::priority_color(&card.priority)),
            ),
        ]),
    ];
    if let Some(m) = &card.milestone {
        lines.push(Line::from(vec![
            field("milestone"),
            Span::styled(m.clone(), Style::default().fg(theme::MILESTONE)),
        ]));
    }
    lines.push(Line::raw(""));
    let mut hints = theme::hints(&[("e", "edit"), ("q/esc", "back")]);
    hints.spans.insert(0, Span::raw("  "));
    lines.push(hints);
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
