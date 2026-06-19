use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let project_chip = match &app.scope.project { Some(k) => format!("▸{k}"), None => "▸all".into() };
    let milestone_chip = match &app.scope.milestone { Some(m) => format!("▸{m}"), None => "—".into() };
    let count = app.scoped_card_count();
    let blocked = app.blocked_count();
    let mut spans = vec![
        Span::styled(project_chip, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "), Span::raw(milestone_chip),
        Span::raw("    "), Span::raw(format!("{count} issues")), Span::raw("    "),
    ];
    let blocked_color = if blocked > 0 { Color::Red } else { Color::DarkGray };
    spans.push(Span::styled(format!("⚠ {blocked} blocked"), Style::default().fg(blocked_color)));
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
        for y in 0..buf.area.height { for x in 0..buf.area.width { s.push_str(buf[(x,y)].symbol()); } s.push('\n'); }
        s
    }

    #[test]
    fn top_bar_shows_scope_count_and_blocked() {
        let mut app = App::new();
        app.scope.set_project(Some("CLI".into()));
        app.cards = vec![Card { id:0, key:"CLI-1".into(), project:"CLI".into(), title:"x".into(),
            status:"blocked".into(), priority:"low".into(), position:1.0, milestone_id:None, milestone:None }];
        let mut t = Terminal::new(TestBackend::new(100,1)).unwrap();
        t.draw(|f| draw(f, Rect::new(0,0,100,1), &app)).unwrap();
        let d = dump(t.backend().buffer());
        assert!(d.contains("▸CLI"), "scope chip:\n{d}");
        assert!(d.contains("1 issues"), "count:\n{d}");
        assert!(d.contains("⚠ 1 blocked"), "blocked:\n{d}");
    }
}
