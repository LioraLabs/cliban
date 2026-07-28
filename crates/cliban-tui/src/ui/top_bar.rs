use super::theme;
use crate::app::App;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    // The project chip wears the project's own color (matching the milestone
    // page), the milestone scope is violet, counts stay quiet, and the
    // blocked tally only turns alarm-red when it has something to say.
    let project_span = match &app.scope.project {
        Some(k) => Span::styled(
            format!("▸{k}"),
            Style::default()
                .fg(theme::project_color(k))
                .add_modifier(Modifier::BOLD),
        ),
        None => Span::styled(
            "▸all".to_string(),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
    };
    let milestone_span = match &app.scope.milestone {
        Some(m) => Span::styled(format!("▸{m}"), Style::default().fg(theme::MILESTONE)),
        None => Span::styled("—".to_string(), Style::default().fg(theme::DIM)),
    };
    let count = app.scoped_card_count();
    let blocked = app.blocked_count();
    let blocked_style = if blocked > 0 {
        Style::default()
            .fg(theme::ALARM)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::DIM)
    };
    let mut spans = vec![
        project_span,
        Span::raw("  "),
        milestone_span,
        Span::raw("    "),
        Span::styled(format!("{count} issues"), Style::default().fg(theme::DIM)),
        Span::raw("    "),
        Span::styled(format!("⚠ {blocked} blocked"), blocked_style),
    ];
    // Unread mail lights up only when someone else did something — `a` opens
    // the mailbox and clears it.
    let unseen = app.unseen_count();
    if unseen > 0 {
        spans.push(Span::raw("    "));
        spans.push(Span::styled(
            format!("✉ {unseen} new"),
            Style::default()
                .fg(theme::MARKER)
                .add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Card};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn dump(buf: &ratatui::buffer::Buffer) -> String {
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
    fn top_bar_shows_scope_count_and_blocked() {
        let mut app = App::new();
        app.scope.set_project(Some("CLI".into()));
        app.cards = vec![Card {
            id: 0,
            key: "CLI-1".into(),
            project: "CLI".into(),
            title: "x".into(),
            status: "blocked".into(),
            priority: "low".into(),
            position: 1.0,
            milestone_id: None,
            milestone: None,
            labels: Vec::new(),
        }];
        let mut t = Terminal::new(TestBackend::new(100, 1)).unwrap();
        t.draw(|f| draw(f, Rect::new(0, 0, 100, 1), &app)).unwrap();
        let d = dump(t.backend().buffer());
        assert!(d.contains("▸CLI"), "scope chip:\n{d}");
        assert!(d.contains("1 issues"), "count:\n{d}");
        assert!(d.contains("⚠ 1 blocked"), "blocked:\n{d}");
        assert!(!d.contains('✉'), "no badge without unread mail:\n{d}");
    }

    #[test]
    fn top_bar_shows_the_mailbox_badge_when_there_is_unread_mail() {
        use crate::app::ActivityRef;
        let mut app = App::new();
        app.self_actor = None;
        app.activity = vec![ActivityRef {
            issue_key: "PULSE-1".into(),
            title: "t".into(),
            project: "PULSE".into(),
            kind: "status".into(),
            message: "backlog → done".into(),
            actor: Some("alex".into()),
            ts: chrono::Utc::now(),
        }];
        app.last_seen = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        let mut t = Terminal::new(TestBackend::new(100, 1)).unwrap();
        t.draw(|f| draw(f, Rect::new(0, 0, 100, 1), &app)).unwrap();
        let d = dump(t.backend().buffer());
        assert!(d.contains("✉ 1 new"), "badge missing:\n{d}");
    }
}
