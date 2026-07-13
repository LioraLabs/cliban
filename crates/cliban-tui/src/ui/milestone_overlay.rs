//! Centered milestone overlay — scrolls and filters like the project
//! `picker`, but renders the richer name/status/target columns and keeps the
//! milestone-specific affordances (`E` edit, `Enter` scope the board to the
//! milestone, `A` toggle open-only vs. all statuses).
//!
//! Layout mirrors `picker::draw`: a query line at the top, a scrolling list in
//! the middle (window slides to keep the cursor visible), and a footer of
//! hints. Filtering is done by `app::filtered_overlay` so the widget stays
//! rendering-only.

use crate::app::{filtered_overlay, MilestoneOverlayState};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, area: Rect, state: &MilestoneOverlayState) {
    let popup = centered_rect(60, 20, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Milestones ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.width == 0 || inner.height < 2 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // query line
            Constraint::Min(0),    // list
            Constraint::Length(1), // footer hints
        ])
        .split(inner);

    // Query line — `> <typed>_`, matching the project picker.
    let query_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Yellow)),
        Span::raw(state.query.as_str()),
        Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
    ]);
    frame.render_widget(Paragraph::new(query_line), chunks[0]);

    // List — visible window of the *filtered* milestones. Scroll math is
    // copied from `picker::draw`: anchor to the top while the cursor fits,
    // otherwise slide so the cursor sits on the last visible row.
    let idx = filtered_overlay(state);
    let list_rect = chunks[1];
    let rows = list_rect.height as usize;
    let total = idx.len();
    let cursor = state.cursor.min(total.saturating_sub(1));
    let start = if cursor < rows { 0 } else { cursor + 1 - rows };
    let end = (start + rows).min(total);

    let lines: Vec<Line> = if total == 0 {
        let msg = if state.items.is_empty() {
            "  (no milestones)"
        } else {
            "  (no matches)"
        };
        vec![Line::styled(msg, Style::default().fg(Color::DarkGray))]
    } else {
        idx[start..end]
            .iter()
            .enumerate()
            .map(|(i, &item_idx)| {
                let m = &state.items[item_idx];
                let target = m.target.clone().unwrap_or_else(|| "-".into());
                let text = format!("{:<18} {:<10} {}", m.name, m.status, target);
                if start + i == cursor {
                    Line::from(vec![
                        Span::styled("▸ ", Style::default().fg(Color::Yellow)),
                        Span::styled(text, Style::default().add_modifier(Modifier::REVERSED)),
                    ])
                } else {
                    Line::from(vec![Span::raw("  "), Span::raw(text)])
                }
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), list_rect);

    // `A` toggles between open-only (default) and all statuses; the hint names
    // the state you'd switch *to*. `enter view` (not "filter") because Enter
    // scopes the board to the milestone rather than narrowing this list.
    let toggle_hint = if state.show_all { "A open" } else { "A all" };
    let footer = Line::styled(
        format!("  j/k move   enter view   {toggle_hint}   E edit   esc close"),
        Style::default().fg(Color::DarkGray),
    );
    frame.render_widget(Paragraph::new(footer), chunks[2]);
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
