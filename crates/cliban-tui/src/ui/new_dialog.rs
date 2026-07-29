//! The quick-create dialog (n/N): a small centered form with just enough
//! fields to get the thing on screen — depth comes later via `e`/`E`.
//!
//! ```text
//! ┌─ New issue · CLI → backlog ─────────────┐
//! │                                         │
//! │  Title  fix the flaky reload test_      │
//! │                                         │
//! │ Enter create  Esc cancel                │
//! └─────────────────────────────────────────┘
//! ```

use super::theme;
use crate::app::{NewDialogState, NewKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, area: Rect, st: &NewDialogState) {
    let width = (area.width * 3 / 5).clamp(30, 64).min(area.width);
    // Border + padding rows + one row per field + footer.
    let height = (st.fields.len() as u16 + 5).min(area.height);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);

    let label_w = st.fields.iter().map(|f| f.label.len()).max().unwrap_or(0);
    let mut lines = vec![Line::raw("")];
    for (i, f) in st.fields.iter().enumerate() {
        let focused = i == st.focus;
        let marker = if focused { "▸ " } else { "  " };
        let mut spans = vec![
            Span::styled(marker, Style::default().fg(theme::marker())),
            Span::styled(
                format!("{:<label_w$}  ", f.label),
                Style::default().fg(theme::accent()),
            ),
            Span::raw(f.value.clone()),
        ];
        if focused {
            spans.push(Span::styled(
                "_",
                Style::default().add_modifier(Modifier::SLOW_BLINK),
            ));
        }
        // The optional field says so — everything else on the form is not.
        if f.value.is_empty() && !focused && is_optional(&st.kind, i) {
            spans.push(Span::styled(
                "(optional)",
                Style::default().fg(theme::dim()),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(match &st.error {
        Some(e) => Line::from(Span::styled(
            format!(" {e}"),
            Style::default().fg(theme::marker()),
        )),
        None => {
            let mut hints = vec![("Enter", "create")];
            if st.fields.len() > 1 {
                hints.push(("Tab", "next field"));
            }
            hints.push(("Esc", "cancel"));
            let mut l = theme::hints(&hints);
            l.spans.insert(0, Span::raw(" "));
            l
        }
    });

    let title = st.title();
    frame.render_widget(
        Paragraph::new(lines).block(theme::popup_block(&title, theme::accent())),
        popup,
    );
}

/// Which fields may be left empty (everything else refuses on Enter).
fn is_optional(kind: &NewKind, field: usize) -> bool {
    matches!(kind, NewKind::Milestone { .. }) && field == 1
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
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
    use crate::app::NewOrigin;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn dump_state(st: &NewDialogState) -> String {
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| draw(f, Rect::new(0, 0, 80, 24), st)).unwrap();
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
    fn issue_dialog_shows_title_field_and_context() {
        let mut st = NewDialogState::issue("CLI".into(), "backlog".into(), String::new());
        st.fields[0].value = "fix the thing".into();
        let d = dump_state(&st);
        assert!(d.contains("New issue · CLI → backlog"), "{d}");
        assert!(d.contains("Title"), "{d}");
        assert!(d.contains("fix the thing"), "{d}");
        assert!(d.contains("Enter create"), "{d}");
        assert!(!d.contains("Tab"), "single field needs no Tab hint:\n{d}");
    }

    #[test]
    fn milestone_dialog_marks_the_target_optional() {
        let st = NewDialogState::milestone("CLI".into(), NewOrigin::Board);
        let d = dump_state(&st);
        assert!(d.contains("New milestone · CLI"), "{d}");
        assert!(d.contains("Name"), "{d}");
        assert!(d.contains("Target"), "{d}");
        assert!(d.contains("(optional)"), "{d}");
        assert!(d.contains("Tab next field"), "{d}");
    }

    #[test]
    fn error_replaces_the_hint_row() {
        let mut st = NewDialogState::project(NewOrigin::Board);
        st.error = Some("key: 2-10 uppercase letters/digits".into());
        let d = dump_state(&st);
        assert!(d.contains("New project"), "{d}");
        assert!(d.contains("key: 2-10 uppercase"), "{d}");
        assert!(!d.contains("Enter create"), "{d}");
    }
}
