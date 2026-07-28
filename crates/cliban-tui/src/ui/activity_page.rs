//! The activity page — a mailbox for the board. Every audit entry (status
//! moves, edits, notes, plan ticks, archives), newest first, filterable by
//! event kind and by typing (key, title, message, actor all match).
//!
//! `closed` is the headline filter: status moves that landed in done —
//! "what shipped recently" — which is a *virtual* kind derived from the
//! entry, not a stored one. Enter jumps the board cursor to the entry's
//! issue when it's visible under the current scope.

use super::theme;
use crate::app::{activity_rows, ActFilter, ActivityPageState, ActivityRef, App};
use cliban_core::time::relative;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// Height of the detail pane, borders included.
const DETAIL_HEIGHT: u16 = 8;

/// Event-kind colors: closed moves get the completion green even though
/// their stored kind is `status` — the mailbox colors by effect.
fn kind_color(e: &ActivityRef) -> Color {
    if e.closes() {
        return theme::milestone_status_color("completed");
    }
    match e.kind.as_str() {
        "status" => Color::Indexed(81),
        "edit" => Color::Indexed(179),
        "log" => theme::milestone_accent(),
        "plan" => Color::Indexed(75),
        "archive" => Color::Indexed(245),
        _ => theme::dim(),
    }
}

fn filter_color(f: ActFilter) -> Color {
    match f {
        ActFilter::All => theme::accent(),
        ActFilter::Closed => theme::milestone_status_color("completed"),
        ActFilter::Moves => Color::Indexed(81),
        ActFilter::Edits => Color::Indexed(179),
        ActFilter::Notes => theme::milestone_accent(),
        ActFilter::Plan => Color::Indexed(75),
        ActFilter::Archive => Color::Indexed(245),
    }
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App, state: &ActivityPageState) {
    frame.render_widget(Clear, area);
    let rows = activity_rows(app, state);
    let cursor = state.cursor.min(rows.len().saturating_sub(1));

    let block = theme::popup_block("Activity", theme::accent()).borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height < 3 {
        return;
    }

    let detail = if inner.height > DETAIL_HEIGHT + 4 {
        DETAIL_HEIGHT
    } else {
        0
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // chips
            Constraint::Length(1),      // query
            Constraint::Min(1),         // list
            Constraint::Length(detail), // detail pane
            Constraint::Length(1),      // footer hints
        ])
        .split(inner);

    frame.render_widget(Paragraph::new(chips(state, rows.len())), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" > ", Style::default().fg(theme::marker())),
            Span::raw(state.query.as_str()),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ])),
        chunks[1],
    );

    draw_list(frame, chunks[2], app, &rows, cursor);
    if detail > 0 {
        let focused = rows.get(cursor).and_then(|&i| app.activity.get(i));
        draw_detail(frame, chunks[3], focused);
    }
    const HINTS: &[(&str, &str)] = &[
        ("↑/↓", "move"),
        ("enter", "jump to issue"),
        ("Tab", "event type"),
        ("esc", "close"),
    ];
    let mut footer = theme::hints(HINTS);
    footer.spans.insert(0, Span::raw("  "));
    if let Some(m) = &app.status_msg {
        let mut spans = vec![
            Span::styled(format!("  {m}"), Style::default().fg(theme::marker())),
            Span::styled("  |", Style::default().fg(theme::dim())),
        ];
        spans.extend(footer.spans);
        footer = Line::from(spans);
    }
    frame.render_widget(Paragraph::new(footer), chunks[4]);
}

/// One chip per event kind, each wearing its kind color when active.
fn chips(state: &ActivityPageState, count: usize) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for &f in ActFilter::ALL {
        let style = if f == state.filter {
            Style::default()
                .fg(Color::Black)
                .bg(filter_color(f))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::dim())
        };
        spans.push(Span::styled(format!(" {} ", f.label()), style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!("   {count} shown"),
        Style::default().fg(theme::dim()),
    ));
    Line::from(spans)
}

fn draw_list(frame: &mut Frame, area: Rect, app: &App, rows: &[usize], cursor: usize) {
    if rows.is_empty() {
        let msg = if app.activity.is_empty() {
            "  nothing recorded yet — moves, edits, and archives land here as they happen"
        } else {
            "  nothing of this kind (Tab switches, / clears with backspace)"
        };
        frame.render_widget(
            Paragraph::new(Line::styled(msg, Style::default().fg(theme::dim()))),
            area,
        );
        return;
    }

    let height = area.height as usize;
    let start = if cursor < height {
        0
    } else {
        cursor + 1 - height
    };
    let end = (start + height).min(rows.len());

    let lines: Vec<Line> = rows[start..end]
        .iter()
        .enumerate()
        .map(|(i, &item)| {
            let selected = start + i == cursor;
            row_line(&app.activity[item], selected, area.width as usize)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// One entry: `▸ 2h ago   PULSE-4  status   backlog → done   · claude`.
fn row_line(e: &ActivityRef, selected: bool, width: usize) -> Line<'static> {
    let base = if selected {
        Style::default().bg(theme::selection_bg())
    } else {
        Style::default()
    };
    let style = |fg: Color| base.fg(fg);

    let marker = if selected {
        Span::styled("▸ ", style(theme::marker()))
    } else {
        Span::styled("  ", base)
    };
    let actor = match &e.actor {
        Some(a) => format!("  · {a}"),
        None => String::new(),
    };
    let mut spans = vec![
        marker,
        Span::styled(
            format!("{:<10} ", relative(e.ts, chrono::Utc::now())),
            style(theme::dim()),
        ),
        Span::styled(
            format!("{:<9} ", truncate(&e.issue_key, 9)),
            style(theme::project_color(&e.project)).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{:<8} ", e.kind), style(kind_color(e))),
        Span::styled(format!("{:<52} ", truncate(&e.message, 52)), base),
        Span::styled(actor, style(theme::dim())),
    ];
    if selected {
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        if width > used {
            spans.push(Span::styled(" ".repeat(width - used), base));
        }
    }
    Line::from(spans)
}

fn draw_detail(frame: &mut Frame, area: Rect, e: Option<&ActivityRef>) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::dim()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(e) = e else {
        return;
    };

    let dim = Style::default().fg(theme::dim());
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" {}", e.issue_key),
                Style::default()
                    .fg(theme::project_color(&e.project))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", e.title),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(format!(" {}", e.kind), Style::default().fg(kind_color(e))),
            Span::styled(format!(" · {} UTC", e.ts.format("%Y-%m-%d %H:%M")), dim),
            Span::styled(
                match &e.actor {
                    Some(a) => format!(" · by {a}"),
                    None => " · unattributed".into(),
                },
                dim,
            ),
        ]),
        Line::raw(""),
        Line::raw(format!(" {}", e.message)),
    ];
    lines.truncate(inner.height as usize);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    s.chars().take(keep).chain(std::iter::once('…')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn entry(key: &str, kind: &str, message: &str) -> ActivityRef {
        ActivityRef {
            issue_key: key.into(),
            title: "Cert expiry checks".into(),
            project: "PULSE".into(),
            kind: kind.into(),
            message: message.into(),
            actor: Some("claude".into()),
            ts: chrono::Utc::now(),
        }
    }

    fn render(app: &App, state: &ActivityPageState) -> String {
        let mut t = Terminal::new(TestBackend::new(120, 24)).unwrap();
        t.draw(|f| draw(f, Rect::new(0, 0, 120, 24), app, state))
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
    fn page_shows_entries_kinds_and_the_focused_detail() {
        let mut app = App::new();
        app.activity = vec![
            entry("PULSE-1", "status", "backlog → done"),
            entry("PULSE-2", "log", "root cause found"),
        ];
        let d = render(&app, &ActivityPageState::default());
        assert!(d.contains("Activity"), "{d}");
        assert!(d.contains("PULSE-1"), "{d}");
        assert!(d.contains("backlog → done"), "{d}");
        assert!(d.contains("root cause found"), "{d}");
        assert!(d.contains("· claude"), "actor missing:\n{d}");
        assert!(d.contains("Cert expiry checks"), "detail title:\n{d}");
        assert!(d.contains("by claude"), "detail actor:\n{d}");
        assert!(d.contains("2 shown"), "{d}");
    }

    #[test]
    fn closed_filter_keeps_only_moves_that_landed_in_done() {
        let mut app = App::new();
        app.activity = vec![
            entry("PULSE-1", "status", "backlog → done"),
            entry("PULSE-2", "status", "backlog → in-progress"),
            entry("PULSE-3", "archive", "archived"),
        ];
        let state = ActivityPageState {
            filter: ActFilter::Closed,
            ..Default::default()
        };
        let d = render(&app, &state);
        assert!(d.contains("PULSE-1"), "{d}");
        assert!(!d.contains("PULSE-2"), "non-done move leaked:\n{d}");
        assert!(!d.contains("PULSE-3"), "archive leaked into closed:\n{d}");
        assert!(d.contains("1 shown"), "{d}");
    }

    #[test]
    fn empty_mailbox_explains_itself() {
        let d = render(&App::new(), &ActivityPageState::default());
        assert!(d.contains("nothing recorded yet"), "{d}");
        for chip in [
            "all", "closed", "moves", "edits", "notes", "plan", "archive",
        ] {
            assert!(d.contains(chip), "missing chip {chip}:\n{d}");
        }
    }

    #[test]
    fn typing_filters_by_actor_too() {
        let mut app = App::new();
        let mut other = entry("TIDE-1", "status", "backlog → done");
        other.actor = Some("alex".into());
        app.activity = vec![entry("PULSE-1", "status", "backlog → done"), other];
        let state = ActivityPageState {
            query: "alex".into(),
            ..Default::default()
        };
        let d = render(&app, &state);
        assert!(d.contains("TIDE-1"), "{d}");
        assert!(!d.contains("PULSE-1"), "{d}");
    }
}
