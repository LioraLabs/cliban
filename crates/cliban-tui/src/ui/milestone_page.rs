//! The milestone page — a full-screen view of every milestone, ordered by
//! recent activity, with a detail pane for the focused one.
//!
//! Four bands: a title bar with the status/sort chips, the query line, the
//! scrolling list, and the detail pane (plus a footer of hints). The page
//! owns no data: rows come from `app.milestones` via `app::page_rows`, so a
//! reload after an edit or status change refreshes it in place.

use crate::app::{page_rows, App, MilestonePageState, MilestoneRef, StatusFilter};
use cliban_core::time::relative;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// Height of the detail pane, borders included.
const DETAIL_HEIGHT: u16 = 8;
/// Width of the progress bar in the list rows.
const BAR_WIDTH: usize = 10;

pub fn draw(frame: &mut Frame, area: Rect, app: &App, state: &MilestonePageState) {
    frame.render_widget(Clear, area);
    let rows = page_rows(app, state);
    let cursor = state.cursor.min(rows.len().saturating_sub(1));

    let title = match &app.scope.project {
        Some(p) => format!(" Milestones · {p} "),
        None => " Milestones · all projects ".to_string(),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height < 3 {
        return;
    }

    // The detail pane only earns its space once the list has room to breathe.
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
            Span::styled(" > ", Style::default().fg(Color::Yellow)),
            Span::raw(state.query.as_str()),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ])),
        chunks[1],
    );

    draw_list(frame, chunks[2], app, &rows, cursor);
    if detail > 0 {
        let focused = rows.get(cursor).and_then(|&i| app.milestones.get(i));
        draw_detail(frame, chunks[3], focused);
    }
    // The page hides the board's status line, so surface `status_msg` here —
    // otherwise a refused action (N with no project scoped) looks like a no-op.
    const HINTS: &str = "j/k move   enter scope board   Tab status   S sort   C cycle status   E edit   N new   esc close";
    let footer = match &app.status_msg {
        Some(m) => format!("  {m}  |  {HINTS}"),
        None => format!("  {HINTS}"),
    };
    frame.render_widget(
        Paragraph::new(Line::styled(footer, Style::default().fg(Color::DarkGray))),
        chunks[4],
    );
}

/// `open  completed  cancelled  all` with the active bucket highlighted, then
/// the sort and the row count.
fn chips(state: &MilestonePageState, count: usize) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for f in [
        StatusFilter::Open,
        StatusFilter::Completed,
        StatusFilter::Cancelled,
        StatusFilter::All,
    ] {
        let style = if f == state.filter {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(format!(" {} ", f.label()), style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!("   sort: {}   {} shown", state.sort.label(), count),
        Style::default().fg(Color::DarkGray),
    ));
    Line::from(spans)
}

fn draw_list(frame: &mut Frame, area: Rect, app: &App, rows: &[usize], cursor: usize) {
    if rows.is_empty() {
        let msg = if app.milestones.is_empty() {
            "  no milestones yet — N creates one"
        } else {
            "  nothing in this bucket (Tab switches, / clears with backspace)"
        };
        frame.render_widget(
            Paragraph::new(Line::styled(msg, Style::default().fg(Color::DarkGray))),
            area,
        );
        return;
    }

    // Same window math as the project picker: anchor at the top until the
    // cursor would fall off the bottom, then slide.
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
            row_line(&app.milestones[item], selected, area.width as usize)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// One list row: `▸ name  PROJ  status  ███░░ 12/18  2026-08-01  2h ago`.
fn row_line(m: &MilestoneRef, selected: bool, width: usize) -> Line<'static> {
    let target = m.target.clone().unwrap_or_else(|| "-".into());
    let text = format!(
        "{:<24} {:<6} {:<10} {} {:>7}  {:<11} {}",
        truncate(&m.name, 24),
        truncate(&m.project, 6),
        m.status,
        bar(m.percent()),
        format!("{}/{}", m.done, m.total),
        target,
        relative(m.last_activity, chrono::Utc::now()),
    );
    let text = truncate(&text, width.saturating_sub(2));
    if selected {
        Line::from(vec![
            Span::styled("▸ ", Style::default().fg(Color::Yellow)),
            Span::styled(text, Style::default().add_modifier(Modifier::REVERSED)),
        ])
    } else {
        Line::from(vec![Span::raw("  "), Span::raw(text)])
    }
}

fn draw_detail(frame: &mut Frame, area: Rect, m: Option<&MilestoneRef>) {
    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(m) = m else {
        return;
    };

    let mut lines = vec![
        Line::from(Span::styled(
            format!(" {}", m.name),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                " {} · {}/{} done ({}%) · {} · target {} · last activity {}",
                m.project,
                m.done,
                m.total,
                m.percent(),
                m.status,
                m.target.clone().unwrap_or_else(|| "none".into()),
                relative(m.last_activity, chrono::Utc::now()),
            ),
            Style::default().fg(Color::DarkGray),
        )),
    ];
    if !m.description.trim().is_empty() {
        lines.push(Line::raw(""));
        for l in m
            .description
            .lines()
            .take(inner.height.saturating_sub(3) as usize)
        {
            lines.push(Line::raw(format!(" {l}")));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// A `█`/`░` completion bar. 0 total renders empty rather than full.
fn bar(percent: u16) -> String {
    let filled = (percent as usize * BAR_WIDTH).div_ceil(100).min(BAR_WIDTH);
    format!("{}{}", "█".repeat(filled), "░".repeat(BAR_WIDTH - filled))
}

/// Truncate to `max` *characters* (not bytes), with an ellipsis when cut.
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
    use crate::app::{MilestonePageState, PageSort};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn ms(name: &str, status: &str, done: i64, total: i64) -> MilestoneRef {
        MilestoneRef {
            id: 1,
            project: "CLI".into(),
            name: name.into(),
            description: "the description".into(),
            status: status.into(),
            target: Some("2026-08-01".into()),
            total,
            done,
            last_activity: chrono::Utc::now(),
        }
    }

    fn render(app: &App, state: &MilestonePageState) -> String {
        let mut t = Terminal::new(TestBackend::new(110, 24)).unwrap();
        t.draw(|f| draw(f, Rect::new(0, 0, 110, 24), app, state))
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
    fn page_shows_progress_target_and_detail_for_the_focused_row() {
        let mut app = App::new();
        app.scope.set_project(Some("CLI".into()));
        app.milestones = vec![ms("v0.3-cutover", "open", 12, 18)];
        let d = render(&app, &MilestonePageState::default());
        assert!(d.contains("Milestones · CLI"), "{d}");
        assert!(d.contains("v0.3-cutover"), "{d}");
        assert!(d.contains("12/18"), "progress counts missing:\n{d}");
        assert!(d.contains("2026-08-01"), "target missing:\n{d}");
        assert!(d.contains("66%"), "detail percentage missing:\n{d}");
        assert!(d.contains("the description"), "detail body missing:\n{d}");
    }

    #[test]
    fn empty_bucket_and_empty_board_say_different_things() {
        let app = App::new();
        let d = render(&app, &MilestonePageState::default());
        assert!(d.contains("no milestones yet"), "{d}");

        let mut app = App::new();
        app.milestones = vec![ms("done-one", "completed", 1, 1)];
        let d = render(&app, &MilestonePageState::default()); // default bucket = open
        assert!(d.contains("nothing in this bucket"), "{d}");
    }

    #[test]
    fn chips_mark_the_active_bucket_and_sort() {
        let mut app = App::new();
        app.milestones = vec![ms("a", "open", 0, 0)];
        let state = MilestonePageState {
            sort: PageSort::Target,
            ..Default::default()
        };
        let d = render(&app, &state);
        assert!(d.contains("sort: target"), "{d}");
        assert!(d.contains("1 shown"), "{d}");
        for label in ["open", "completed", "cancelled", "all"] {
            assert!(d.contains(label), "missing bucket chip {label}:\n{d}");
        }
    }

    #[test]
    fn bar_is_empty_at_zero_and_full_at_hundred() {
        assert_eq!(bar(0), "░".repeat(BAR_WIDTH));
        assert_eq!(bar(100), "█".repeat(BAR_WIDTH));
        // Any nonzero progress shows at least one block.
        assert!(bar(1).starts_with('█'));
    }

    #[test]
    fn truncate_counts_characters_not_bytes() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("ééééé", 3), "éé…");
    }

    #[test]
    fn footer_surfaces_the_status_message() {
        let mut app = App::new();
        app.status_msg = Some("scope a project (p) first".into());
        let d = render(&app, &MilestonePageState::default());
        assert!(d.contains("scope a project (p) first"), "{d}");
        // Hints follow the message (and are clipped by the terminal width,
        // exactly like the board's status line).
        assert!(d.contains("|  j/k move"), "hints follow the message:\n{d}");
        assert!(
            render(&App::new(), &MilestonePageState::default()).contains("esc close"),
            "full hints show when there is no message"
        );
    }

    #[test]
    fn narrow_terminal_still_renders_without_panicking() {
        let mut app = App::new();
        app.milestones = vec![ms("v0.3-cutover", "open", 1, 2)];
        let mut t = Terminal::new(TestBackend::new(24, 6)).unwrap();
        t.draw(|f| {
            draw(
                f,
                Rect::new(0, 0, 24, 6),
                &app,
                &MilestonePageState::default(),
            )
        })
        .unwrap();
    }
}
