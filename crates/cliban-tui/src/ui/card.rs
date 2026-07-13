use crate::app::Card;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

fn priority_color(p: &str) -> Color {
    match p {
        "urgent" => Color::Indexed(196),
        "high" => Color::Indexed(208),
        "medium" => Color::Indexed(226),
        "low" => Color::Indexed(33),
        _ => Color::DarkGray,
    }
}
fn priority_letter(p: &str) -> &'static str {
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
    let border_style = if is_focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(priority_color(&card.priority))
    };
    let (key_line, title_line) = card_lines(card);
    let title_inner = title_line.strip_prefix("  ").unwrap_or(&title_line);
    let viewport = area.width.saturating_sub(4) as usize;
    let display = if is_focused && title_inner.chars().count() > viewport {
        marquee_slice(title_inner, viewport, now_ms)
    } else {
        truncate(title_inner, viewport)
    };
    let lines = vec![
        Line::from(Span::styled(
            key_line,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  {}", display)),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
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
        }
    }

    #[test]
    fn card_key_line_shows_priority_letter() {
        let (k, t) = card_lines(&card("CLI-8", "high"));
        assert_eq!(k, "CLI-8 (H)");
        assert_eq!(t, "  Hello");
    }

    #[test]
    fn priority_palette_matches_cliban() {
        assert_eq!(priority_color("urgent"), Color::Indexed(196));
        assert_eq!(priority_color("high"), Color::Indexed(208));
        assert_eq!(priority_color("medium"), Color::Indexed(226));
        assert_eq!(priority_color("low"), Color::Indexed(33));
    }

    #[test]
    fn focused_border_cyan_unfocused_border_priority() {
        let mut t = Terminal::new(TestBackend::new(30, 4)).unwrap();
        t.draw(|f| draw_card(f, Rect::new(0, 0, 30, 4), &card("CLI-8", "urgent"), true, 0))
            .unwrap();
        assert_eq!(t.backend().buffer()[(0, 0)].fg, Color::Cyan);
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
