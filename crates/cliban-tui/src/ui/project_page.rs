//! The project page — a full-screen view of every project, ordered by
//! recent activity, with a detail pane for the focused one.
//!
//! The milestone page's twin: chips (active/archived/all), the query line,
//! the scrolling list, the detail pane, and a footer of hints. Rows are
//! projected from `app.projects` by `app::project_rows`, so a reload after
//! an edit, create, or archive refreshes the page in place.
//!
//! This page is also where projects are *managed*: `N` creates one (the
//! only way to do so from the TUI — the empty-database cold start goes
//! through here), `E` edits, and `A` archives or unarchives with a confirm.

use super::theme;
use crate::app::{project_rows, App, ProjFilter, ProjectPageState, ProjectRef};
use cliban_core::time::relative;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// Height of the detail pane, borders included.
const DETAIL_HEIGHT: u16 = 8;
/// Width of the issues progress bar in the list rows.
const BAR_WIDTH: usize = 10;
/// Width of the wide progress bar in the detail pane.
const DETAIL_BAR_WIDTH: usize = 30;

pub fn draw(frame: &mut Frame, area: Rect, app: &App, state: &ProjectPageState) {
    frame.render_widget(Clear, area);
    let rows = project_rows(app, state);
    let cursor = state.cursor.min(rows.len().saturating_sub(1));

    let block = theme::popup_block("Projects", theme::accent()).borders(Borders::ALL);
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
        let focused = rows.get(cursor).and_then(|&i| app.projects.get(i));
        draw_detail(frame, chunks[3], focused);
    }
    const HINTS: &[(&str, &str)] = &[
        ("↑/↓", "move"),
        ("enter", "scope board"),
        ("Tab", "bucket"),
        ("S", "sort"),
        ("A", "archive"),
        ("E", "edit"),
        ("N", "new"),
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

/// `active  archived  all` with the active bucket highlighted, then the sort
/// and the row count.
fn chips(state: &ProjectPageState, count: usize) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for f in [ProjFilter::Active, ProjFilter::Archived, ProjFilter::All] {
        let accent = match f {
            ProjFilter::Active => theme::milestone_status_color("open"),
            ProjFilter::Archived => theme::milestone_status_color("cancelled"),
            ProjFilter::All => theme::accent(),
        };
        let style = if f == state.filter {
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::dim())
        };
        spans.push(Span::styled(format!(" {} ", f.label()), style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!("   sort: {}   {} shown", state.sort.label(), count),
        Style::default().fg(theme::dim()),
    ));
    Line::from(spans)
}

fn draw_list(frame: &mut Frame, area: Rect, app: &App, rows: &[usize], cursor: usize) {
    if rows.is_empty() {
        let msg = if app.projects.is_empty() {
            "  no projects yet — N creates one (that's how a fresh board starts)"
        } else {
            "  nothing in this bucket (Tab switches, / clears with backspace)"
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
            row_line(&app.projects[item], selected, area.width as usize)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// One list row: `▸ KEY  name  ███░░ 12/18  3 ms  2h ago  archived`.
/// Archived rows dim their name so the working set pops in the all bucket.
fn row_line(p: &ProjectRef, selected: bool, width: usize) -> Line<'static> {
    let base = if selected {
        Style::default().bg(theme::selection_bg())
    } else {
        Style::default()
    };
    let style = |fg: Color| base.fg(fg);

    let percent = percent(p);
    let (filled, empty) = bar_parts(percent, BAR_WIDTH);
    let name_style = if p.archived {
        style(theme::dim()).add_modifier(Modifier::BOLD)
    } else {
        base.add_modifier(Modifier::BOLD)
    };
    let marker = if selected {
        Span::styled("▸ ", style(theme::marker()))
    } else {
        Span::styled("  ", base)
    };
    let mut spans = vec![
        marker,
        Span::styled(
            format!("{:<7} ", truncate(&p.key, 7)),
            style(theme::project_color(&p.key)).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{:<24} ", truncate(&p.name, 24)), name_style),
        Span::styled(filled, style(bar_color(percent))),
        Span::styled(format!("{empty} "), style(theme::dim())),
        Span::styled(format!("{:>7}  ", format!("{}/{}", p.done, p.total)), base),
        Span::styled(
            format!("{:>2} ms  ", p.milestones),
            style(theme::milestone_accent()),
        ),
        Span::styled(
            format!("{:<12}", relative(p.last_activity, chrono::Utc::now())),
            style(theme::dim()),
        ),
        Span::styled(
            if p.archived { "archived" } else { "" }.to_string(),
            style(theme::milestone_status_color("cancelled")),
        ),
    ];
    if selected {
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        if width > used {
            spans.push(Span::styled(" ".repeat(width - used), base));
        }
    }
    Line::from(spans)
}

fn draw_detail(frame: &mut Frame, area: Rect, p: Option<&ProjectRef>) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::dim()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(p) = p else {
        return;
    };

    let dim = Style::default().fg(theme::dim());
    let pct = percent(p);
    let (filled, empty) = bar_parts(pct, DETAIL_BAR_WIDTH);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" {}", p.key),
                Style::default()
                    .fg(theme::project_color(&p.key))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", p.name),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {}/{} issues done ({pct}%) · ", p.done, p.total),
                dim,
            ),
            Span::styled(format!("{} milestones", p.milestones), dim),
            Span::styled(" · ", dim),
            Span::styled(
                if p.archived { "archived" } else { "active" }.to_string(),
                Style::default().fg(theme::milestone_status_color(if p.archived {
                    "cancelled"
                } else {
                    "open"
                })),
            ),
            Span::styled(
                format!(
                    " · last activity {}",
                    relative(p.last_activity, chrono::Utc::now())
                ),
                dim,
            ),
        ]),
        Line::from(vec![
            Span::styled(format!(" {filled}"), Style::default().fg(bar_color(pct))),
            Span::styled(empty, dim),
            Span::styled(format!(" {pct}%"), dim),
        ]),
    ];
    if !p.description.trim().is_empty() {
        lines.push(Line::raw(""));
        for l in p
            .description
            .lines()
            .take(inner.height.saturating_sub(4) as usize)
        {
            lines.push(Line::raw(format!(" {l}")));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn percent(p: &ProjectRef) -> u16 {
    if p.total == 0 {
        0
    } else {
        ((p.done * 100) / p.total) as u16
    }
}

fn bar_parts(percent: u16, width: usize) -> (String, String) {
    let filled = (percent as usize * width).div_ceil(100).min(width);
    ("█".repeat(filled), "░".repeat(width - filled))
}

fn bar_color(percent: u16) -> Color {
    if percent >= 100 {
        theme::milestone_status_color("completed")
    } else {
        theme::milestone_status_color("open")
    }
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
    use crate::app::ProjSort;

    fn proj(key: &str, name: &str, archived: bool, done: i64, total: i64) -> ProjectRef {
        ProjectRef {
            key: key.into(),
            name: name.into(),
            description: "Self-hosted uptime and status pages".into(),
            archived,
            total,
            done,
            milestones: 3,
            last_activity: chrono::Utc::now(),
        }
    }

    fn render(app: &App, state: &ProjectPageState) -> String {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
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
    fn page_shows_rollups_and_detail_for_the_focused_row() {
        let mut app = App::new();
        app.projects = vec![proj("PULSE", "Pulse", false, 5, 16)];
        let d = render(&app, &ProjectPageState::default());
        assert!(d.contains("Projects"), "{d}");
        assert!(d.contains("PULSE"), "{d}");
        assert!(d.contains("Pulse"), "{d}");
        assert!(d.contains("5/16"), "issue counts missing:\n{d}");
        assert!(d.contains("3 ms"), "milestone count missing:\n{d}");
        assert!(d.contains("(31%)"), "detail percentage missing:\n{d}");
        assert!(
            d.contains("Self-hosted uptime"),
            "description missing:\n{d}"
        );
    }

    #[test]
    fn archived_bucket_is_a_tab_away_and_rows_say_archived() {
        let mut app = App::new();
        app.projects = vec![
            proj("PULSE", "Pulse", false, 0, 1),
            proj("OLD", "Old thing", true, 1, 1),
        ];
        // Default bucket hides the archived project…
        let d = render(&app, &ProjectPageState::default());
        assert!(d.contains("PULSE"), "{d}");
        assert!(
            !d.contains("Old thing"),
            "archived leaked into active:\n{d}"
        );
        // …and the archived bucket shows it, labelled.
        let state = ProjectPageState {
            filter: ProjFilter::Archived,
            ..Default::default()
        };
        let d = render(&app, &state);
        assert!(d.contains("Old thing"), "{d}");
        assert!(d.contains("archived"), "{d}");
    }

    #[test]
    fn empty_database_points_at_n() {
        let d = render(&App::new(), &ProjectPageState::default());
        assert!(d.contains("no projects yet"), "{d}");
        assert!(d.contains("N creates one"), "{d}");
    }

    #[test]
    fn chips_mark_the_bucket_and_sort() {
        let mut app = App::new();
        app.projects = vec![proj("PULSE", "Pulse", false, 0, 0)];
        let state = ProjectPageState {
            sort: ProjSort::Name,
            ..Default::default()
        };
        let d = render(&app, &state);
        assert!(d.contains("sort: name"), "{d}");
        assert!(d.contains("1 shown"), "{d}");
        for label in ["active", "archived", "all"] {
            assert!(d.contains(label), "missing bucket chip {label}:\n{d}");
        }
    }
}
