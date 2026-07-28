use super::card::priority_letter;
use super::theme;
use crate::app::{Card, RelationRef};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, area: Rect, card: &Card, relations: &[RelationRef]) {
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
    if !card.labels.is_empty() {
        // Same stable string→color hash the project tags use, so `bug` is
        // the same color on every card.
        let mut spans = vec![field("labels")];
        for (i, l) in card.labels.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                l.clone(),
                Style::default().fg(theme::project_color(l)),
            ));
        }
        lines.push(Line::from(spans));
    }
    // Relations, blockers first: an open blocker is the one thing this
    // popup should shout about — it wears the alarm red the blocked column
    // uses; a resolved blocker fades to the completion green.
    for r in relations {
        let label = match r.kind.as_str() {
            "blocked_by" => "blocked by",
            "blocks" => "blocks",
            _ => "related",
        };
        let color = if r.open_blocker() {
            theme::ALARM
        } else if r.kind == "blocked_by" {
            Color::Indexed(78)
        } else {
            theme::DIM
        };
        lines.push(Line::from(vec![
            field(label),
            Span::styled(
                format!("{}  {}", r.key, truncate(&r.title, 40)),
                Style::default().fg(color),
            ),
        ]));
    }
    lines.push(Line::raw(""));
    let has_open_blocker = relations.iter().any(|r| r.open_blocker());
    let mut hints = if has_open_blocker {
        theme::hints(&[("b", "jump to blocker"), ("e", "edit"), ("q/esc", "back")])
    } else {
        theme::hints(&[("e", "edit"), ("q/esc", "back")])
    };
    hints.spans.insert(0, Span::raw("  "));
    lines.push(hints);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    s.chars().take(keep).chain(std::iter::once('…')).collect()
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

    fn card() -> Card {
        Card {
            id: 0,
            key: "CLI-8".into(),
            project: "CLI".into(),
            title: "Build TUI".into(),
            status: "backlog".into(),
            priority: "high".into(),
            position: 1.0,
            milestone_id: None,
            milestone: None,
            labels: Vec::new(),
        }
    }

    fn render(relations: &[RelationRef]) -> String {
        let c = card();
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| draw(f, Rect::new(0, 0, 80, 24), &c, relations))
            .unwrap();
        let buf = t.backend().buffer();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn detail_renders_key_and_title() {
        let s = render(&[]);
        assert!(s.contains("CLI-8"));
        assert!(s.contains("Build TUI"));
        assert!(!s.contains("blocked by"), "no relation rows without edges");
        assert!(!s.contains("jump to blocker"), "no blocker hint either");
    }

    #[test]
    fn detail_lists_labels_as_colored_chips() {
        let mut c = card();
        c.labels = vec!["bug".into(), "regression".into()];
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| draw(f, Rect::new(0, 0, 80, 24), &c, &[]))
            .unwrap();
        let buf = t.backend().buffer();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        assert!(s.contains("labels"), "{s}");
        assert!(s.contains("bug  regression"), "{s}");
    }

    #[test]
    fn detail_lists_relations_and_offers_the_blocker_jump() {
        let rels = vec![
            RelationRef {
                kind: "blocked_by".into(),
                key: "PULSE-4".into(),
                title: "Flap detection".into(),
                status: "in-progress".into(),
            },
            RelationRef {
                kind: "blocks".into(),
                key: "PULSE-15".into(),
                title: "SQLite persistence".into(),
                status: "backlog".into(),
            },
        ];
        let s = render(&rels);
        assert!(s.contains("blocked by"), "{s}");
        assert!(s.contains("PULSE-4  Flap detection"), "{s}");
        assert!(s.contains("blocks"), "{s}");
        assert!(s.contains("PULSE-15"), "{s}");
        assert!(s.contains("jump to blocker"), "open blocker → b hint:\n{s}");
    }
}
