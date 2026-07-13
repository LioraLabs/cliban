//! Reusable centered-popup picker for cliban.
//!
//! Shared by `Mode::ProjectPicker` and `Mode::MilestonePicker`. The widget
//! itself is rendering-only — filtering is done by the caller (the picker
//! arms in `app::update` filter `items` against `query` before passing them
//! here). This keeps the widget pure and the filter logic testable in
//! isolation.
//!
//! Layout (top to bottom):
//!
//! ```text
//! ┌─ <title> ───────────────────────────┐
//! │ > <query>_                          │
//! │ ─────────────────────────────────── │
//! │ ▸ first item (cursor row, reverse)  │
//! │   second item                       │
//! │   third item                        │
//! │ ...                                 │
//! │ ─────────────────────────────────── │
//! │ Enter select  Esc cancel            │
//! └─────────────────────────────────────┘
//! ```

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// View-model handed to `draw`. All fields are borrowed so the caller owns
/// storage and we don't double-allocate per frame.
///
/// `items` should already be filtered down to the rows the user should see;
/// the widget does no further filtering. `cursor` is an index into `items`
/// (NOT into the unfiltered source list).
pub struct PickerView<'a> {
    pub title: &'a str,
    pub query: &'a str,
    pub items: &'a [String],
    pub cursor: usize,
}

pub fn draw(frame: &mut Frame, area: Rect, view: PickerView) {
    // Centered popup: 60% width, up to 20 rows tall (or smaller for tiny
    // terminals). The list takes the slack between the query row at top and
    // the footer row at bottom.
    let popup = centered_rect(60, 20, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(format!(" {} ", view.title))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(block, popup);

    // Inner rect (inside the border): 1-cell margin all around.
    let inner = Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    if inner.width == 0 || inner.height < 2 {
        // Degenerate terminal — render nothing past the block frame.
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

    // Query line — `> <typed>_` with the caret rendered as a visible
    // underscore so users see where the cursor is.
    let query_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Yellow)),
        Span::raw(view.query),
        Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
    ]);
    frame.render_widget(Paragraph::new(query_line), chunks[0]);

    // List — render visible window. The list rect is `chunks[1]`. Scroll the
    // window so the cursor stays in view: when the cursor would fall off the
    // bottom edge, slide the window down so the cursor is the last row;
    // when it would fall off the top, slide so the cursor is the first row.
    let list_rect = chunks[1];
    let rows = list_rect.height as usize;
    let total = view.items.len();
    let cursor = view.cursor.min(total.saturating_sub(1).max(0));
    // Window start: anchor to 0 while the cursor still fits inside the
    // visible rows (covers both "shorter than viewport" and "scrolled to
    // the top" cases — the two arms collapse intentionally). Otherwise
    // slide the window so the cursor sits on the last visible row.
    let start = if cursor < rows { 0 } else { cursor + 1 - rows };
    let end = (start + rows).min(total);

    let lines: Vec<Line> = if total == 0 {
        vec![Line::styled(
            "  (no matches)",
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        view.items[start..end]
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let abs = start + i;
                if abs == cursor {
                    Line::from(vec![
                        Span::styled("▸ ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            label.as_str(),
                            Style::default().add_modifier(Modifier::REVERSED),
                        ),
                    ])
                } else {
                    Line::from(vec![Span::raw("  "), Span::raw(label.as_str())])
                }
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), list_rect);

    // Footer hints — match the convention used by `help.rs` / `confirm.rs`.
    let footer = Line::from(vec![
        Span::styled("Enter", Style::default().fg(Color::Green)),
        Span::raw(" select  "),
        Span::styled("Esc", Style::default().fg(Color::Red)),
        Span::raw(" cancel"),
    ]);
    frame.render_widget(Paragraph::new(footer), chunks[2]);
}

/// Case-insensitive substring fuzzy filter. Returns the indices into the
/// source vec that match. Caller maps those indices back into whatever
/// payload list they need (e.g. `Vec<ProjectChip>` for `Enter` resolution).
///
/// Empty queries match everything. Used by the picker update arms in
/// `app::update`; lives here next to the rendering so the contract stays in
/// one place.
pub fn fuzzy_indices(items: &[String], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..items.len()).collect();
    }
    let needle = query.to_lowercase();
    items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            if s.to_lowercase().contains(&needle) {
                Some(i)
            } else {
                None
            }
        })
        .collect()
}

fn centered_rect(width_pct: u16, height_max: u16, area: Rect) -> Rect {
    // Width is a percentage of the parent; height is a fixed-but-clamped
    // value so the popup doesn't dominate huge terminals.
    let w = (area.width * width_pct / 100).max(20).min(area.width);
    let h = height_max.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn dump(t: &Terminal<TestBackend>) -> String {
        let buf = t.backend().buffer().clone();
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
    fn picker_renders_title_query_items_and_footer() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let items = vec!["LOOM  Loom".into(), "COOK  Cook".into()];
        terminal
            .draw(|f| {
                draw(
                    f,
                    Rect::new(0, 0, 80, 24),
                    PickerView {
                        title: "Pick project",
                        query: "lo",
                        items: &items,
                        cursor: 0,
                    },
                )
            })
            .unwrap();
        let s = dump(&terminal);
        assert!(s.contains("Pick project"), "title missing");
        assert!(s.contains("> lo"), "query line missing");
        assert!(s.contains("LOOM"), "first item missing");
        assert!(s.contains("COOK"), "second item missing");
        assert!(s.contains("Enter"), "footer missing");
        assert!(s.contains("Esc"), "footer missing");
    }

    #[test]
    fn picker_shows_cursor_marker_on_selected_row() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let items = vec!["one".into(), "two".into(), "three".into()];
        terminal
            .draw(|f| {
                draw(
                    f,
                    Rect::new(0, 0, 80, 24),
                    PickerView {
                        title: "Pick",
                        query: "",
                        items: &items,
                        cursor: 1,
                    },
                )
            })
            .unwrap();
        let s = dump(&terminal);
        // The `▸` marker sits next to the cursor row only.
        let two_line = s
            .lines()
            .find(|l| l.contains("two"))
            .expect("expected line with 'two'");
        assert!(
            two_line.contains('▸'),
            "cursor row should display the ▸ marker"
        );
        let one_line = s
            .lines()
            .find(|l| l.contains("one"))
            .expect("expected line with 'one'");
        assert!(
            !one_line.contains('▸'),
            "non-cursor row must not display the marker"
        );
    }

    #[test]
    fn picker_renders_no_matches_when_items_empty() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(
                    f,
                    Rect::new(0, 0, 80, 24),
                    PickerView {
                        title: "Pick",
                        query: "zzz",
                        items: &[],
                        cursor: 0,
                    },
                )
            })
            .unwrap();
        let s = dump(&terminal);
        assert!(s.contains("(no matches)"));
    }

    #[test]
    fn fuzzy_indices_empty_query_matches_all() {
        let items: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        assert_eq!(fuzzy_indices(&items, ""), vec![0, 1, 2]);
    }

    #[test]
    fn fuzzy_indices_substring_case_insensitive() {
        let items: Vec<String> = vec!["LOOM  Loom".into(), "COOK  Cook".into()];
        assert_eq!(fuzzy_indices(&items, "loom"), vec![0]);
        assert_eq!(fuzzy_indices(&items, "LOOM"), vec![0]);
        assert_eq!(fuzzy_indices(&items, "oo"), vec![0, 1]);
        assert_eq!(fuzzy_indices(&items, "zzz"), Vec::<usize>::new());
    }
}
