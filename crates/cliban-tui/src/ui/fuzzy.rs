//! `/` fuzzy-find overlay. Centered popup; type a substring to jump focus to a
//! matching visible card. Filtering happens in `app::update::fuzzy_search`;
//! this only renders the supplied query + result list, resolving keys against
//! `app.cards`.
use super::theme;
use crate::app::{App, FuzzyState};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, area: Rect, app: &App, state: &FuzzyState) {
    let popup = centered_rect(60, 20, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(theme::popup_block("Fuzzy find", theme::accent()), popup);
    let inner = Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    if inner.width == 0 || inner.height < 2 {
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);
    let query_line = Line::from(vec![
        Span::styled("/ ", Style::default().fg(theme::marker())),
        Span::raw(state.query.as_str()),
        Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
    ]);
    frame.render_widget(Paragraph::new(query_line), chunks[0]);
    let labels: Vec<String> = state
        .results
        .iter()
        .map(|key| match app.cards.iter().find(|c| &c.key == key) {
            Some(c) => format!("{}  {}", c.key, c.title),
            None => key.clone(),
        })
        .collect();
    let list_rect = chunks[1];
    let rows = list_rect.height as usize;
    let total = labels.len();
    let cursor = state.cursor.min(total.saturating_sub(1).max(0));
    let start = if cursor < rows { 0 } else { cursor + 1 - rows };
    let end = (start + rows).min(total);
    let lines: Vec<Line> = if total == 0 {
        vec![Line::styled(
            "  (no matches)",
            Style::default().fg(theme::dim()),
        )]
    } else {
        labels[start..end]
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let abs = start + i;
                if abs == cursor {
                    theme::selected_row(label, list_rect.width as usize)
                } else {
                    Line::from(vec![Span::raw("  "), Span::raw(label.as_str())])
                }
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), list_rect);
    frame.render_widget(
        Paragraph::new(theme::hints(&[("Enter", "jump"), ("Esc", "cancel")])),
        chunks[2],
    );
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
