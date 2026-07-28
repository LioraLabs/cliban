use super::theme;
use crate::app::Card;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

pub fn priority_letter(p: &str) -> &'static str {
    match p {
        "low" => "(L)",
        "medium" => "(M)",
        "high" => "(H)",
        "urgent" => "(U)",
        _ => "( )",
    }
}

pub fn card_lines(card: &Card) -> (String, String) {
    (
        format!("{} {}", card.key, priority_letter(&card.priority)),
        format!("  {}", card.title),
    )
}

pub fn draw_card(frame: &mut Frame, area: Rect, card: &Card, is_focused: bool, now_ms: u128) {
    let prio = theme::priority_color(&card.priority);
    let border_style = if is_focused {
        Style::default()
            .fg(theme::accent())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(prio)
    };
    let title_inner = card.title.as_str();
    let viewport = area.width.saturating_sub(4) as usize;
    let display = if is_focused && title_inner.chars().count() > viewport {
        marquee_slice(title_inner, viewport, now_ms)
    } else {
        truncate(title_inner, viewport)
    };
    // Key bold, priority letter in the priority color — the letter and the
    // border tell the same story even when the border is busy showing focus.
    // Labels ride the key line as small hash-colored tags; two at most, the
    // detail popup lists the rest.
    let mut key_spans = vec![
        Span::styled(
            card.key.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", priority_letter(&card.priority)),
            Style::default().fg(prio).add_modifier(Modifier::BOLD),
        ),
    ];
    for l in card.labels.iter().take(2) {
        key_spans.push(Span::styled(
            format!(" ·{l}"),
            Style::default().fg(theme::project_color(l)),
        ));
    }
    let lines = vec![Line::from(key_spans), Line::from(format!("  {}", display))];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut o: String = s.chars().take(max.saturating_sub(1)).collect();
        o.push('…');
        o
    }
}

pub fn marquee_slice(text: &str, viewport: usize, now_ms: u128) -> String {
    const STEP_MS: u128 = 200;
    const SEP: &str = "   •   ";
    if viewport == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= viewport {
        return text.to_string();
    }
    let padded: Vec<char> = chars
        .iter()
        .chain(SEP.chars().collect::<Vec<_>>().iter())
        .copied()
        .collect();
    let len = padded.len();
    let offset = ((now_ms / STEP_MS) as usize) % len;
    (0..viewport).map(|i| padded[(offset + i) % len]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    fn card(key: &str, prio: &str) -> Card {
        Card {
            id: 0,
            key: key.into(),
            project: "CLI".into(),
            title: "Hello".into(),
            status: "backlog".into(),
            priority: prio.into(),
            position: 1.0,
            milestone_id: None,
            milestone: None,
            labels: Vec::new(),
        }
    }

    #[test]
    fn card_key_line_shows_priority_letter() {
        let (k, t) = card_lines(&card("CLI-8", "high"));
        assert_eq!(k, "CLI-8 (H)");
        assert_eq!(t, "  Hello");
    }

    #[test]
    fn card_key_line_carries_up_to_two_label_tags() {
        let mut c = card("PULSE-8", "high");
        c.labels = vec!["bug".into(), "regression".into(), "backend".into()];
        let mut t = Terminal::new(TestBackend::new(40, 4)).unwrap();
        t.draw(|f| draw_card(f, Rect::new(0, 0, 40, 4), &c, false, 0))
            .unwrap();
        let buf = t.backend().buffer();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        assert!(s.contains("·bug"), "{s}");
        assert!(s.contains("·regression"), "{s}");
        assert!(
            !s.contains("·backend"),
            "third label stays in the popup:\n{s}"
        );
    }

    #[test]
    fn focused_border_cyan_unfocused_border_priority() {
        let mut t = Terminal::new(TestBackend::new(30, 4)).unwrap();
        t.draw(|f| draw_card(f, Rect::new(0, 0, 30, 4), &card("CLI-8", "urgent"), true, 0))
            .unwrap();
        assert_eq!(t.backend().buffer()[(0, 0)].fg, theme::accent());
        let mut t2 = Terminal::new(TestBackend::new(30, 4)).unwrap();
        t2.draw(|f| {
            draw_card(
                f,
                Rect::new(0, 0, 30, 4),
                &card("CLI-8", "urgent"),
                false,
                0,
            )
        })
        .unwrap();
        assert_eq!(t2.backend().buffer()[(0, 0)].fg, Color::Indexed(196));
    }
}
