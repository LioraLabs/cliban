//! The cliban look, in one place.
//!
//! Every screen pulls its colors and chrome from here so the board, the
//! milestone page, and the popups read as one application instead of five
//! widgets that happen to share a binary. The palette sticks to 256-color
//! indexes (never truecolor) so it survives tmux, SSH sessions, and the
//! screenshot pipeline unchanged.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

/// Focus and chrome accent — the one color that always means "you are here".
pub const ACCENT: Color = Color::Cyan;
/// De-emphasised text: hints, counts, metadata.
pub const DIM: Color = Color::DarkGray;
/// Background wash behind the selected row of any list.
pub const SELECTION_BG: Color = Color::Indexed(237);
/// The cursor marker (`▸`) and input carets.
pub const MARKER: Color = Color::Yellow;
/// Alarm red — overdue targets, blocked counts.
pub const ALARM: Color = Color::Indexed(196);
/// Milestone references (scope chip, detail field) — violet, everywhere.
pub const MILESTONE: Color = Color::Indexed(141);

/// Priority palette, shared by card borders, priority letters, and the
/// detail popup. Loud on purpose: priority is the one thing the board
/// should shout about.
pub fn priority_color(p: &str) -> Color {
    match p {
        "urgent" => Color::Indexed(196),
        "high" => Color::Indexed(208),
        "medium" => Color::Indexed(226),
        "low" => Color::Indexed(33),
        _ => DIM,
    }
}

/// Column accents: each board column gets a hue so headers scan at a glance
/// — muted cousins of the priority palette rather than competitors to it.
pub fn column_color(status: &str) -> Color {
    match status {
        "backlog" => Color::Indexed(110),
        "in-progress" => Color::Indexed(81),
        "blocked" => Color::Indexed(203),
        "in-review" => Color::Indexed(141),
        "done" => Color::Indexed(78),
        _ => DIM,
    }
}

/// Milestone lifecycle colors: open is active-blue, completed is green,
/// cancelled fades to gray.
pub fn milestone_status_color(status: &str) -> Color {
    match status {
        "open" => Color::Indexed(81),
        "completed" => Color::Indexed(78),
        "cancelled" => Color::Indexed(245),
        _ => DIM,
    }
}

/// Stable per-project color, hashed from the key so `TIDE` is the same
/// color on every screen and every run. The palette deliberately avoids
/// the priority hues so a project tag never reads as an urgency signal.
pub fn project_color(name: &str) -> Color {
    const PALETTE: [Color; 6] = [
        Color::Indexed(75),  // blue
        Color::Indexed(141), // violet
        Color::Indexed(80),  // teal
        Color::Indexed(168), // pink
        Color::Indexed(117), // sky
        Color::Indexed(179), // tan
    ];
    let h = name
        .bytes()
        .fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    PALETTE[h % PALETTE.len()]
}

/// How urgently a `YYYY-MM-DD` target date should glow, reusing the
/// priority ramp: past-due red, this-week orange, this-month yellow.
/// Inactive rows (completed/cancelled) and unparsable targets stay dim.
pub fn deadline_color(target: Option<&str>, active: bool, today: chrono::NaiveDate) -> Color {
    if !active {
        return DIM;
    }
    let Some(d) = target.and_then(|t| chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d").ok()) else {
        return DIM;
    };
    let days = (d - today).num_days();
    if days < 0 {
        ALARM
    } else if days <= 7 {
        Color::Indexed(208)
    } else if days <= 30 {
        Color::Indexed(226)
    } else {
        DIM
    }
}

/// Rounded popup frame with a bold title in the given accent.
pub fn popup_block(title: &str, accent: Color) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
}

/// `key label  key label …` hint line: keys in the accent, labels dimmed,
/// so the eye can pick out the keys without reading the whole line.
pub fn hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::with_capacity(pairs.len() * 3);
    for (i, (key, label)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            (*key).to_string(),
            Style::default().fg(ACCENT),
        ));
        spans.push(Span::styled(format!(" {label}"), Style::default().fg(DIM)));
    }
    Line::from(spans)
}

/// The selected list row: yellow marker, bold label, and a background wash
/// padded to the full width so the row reads as one bar.
pub fn selected_row(label: &str, width: usize) -> Line<'static> {
    let base = Style::default().bg(SELECTION_BG);
    let mut spans = vec![
        Span::styled("▸ ", base.fg(MARKER)),
        Span::styled(label.to_string(), base.add_modifier(Modifier::BOLD)),
    ];
    let used = 2 + label.chars().count();
    if width > used {
        spans.push(Span::styled(" ".repeat(width - used), base));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn priority_palette_matches_cliban() {
        assert_eq!(priority_color("urgent"), Color::Indexed(196));
        assert_eq!(priority_color("high"), Color::Indexed(208));
        assert_eq!(priority_color("medium"), Color::Indexed(226));
        assert_eq!(priority_color("low"), Color::Indexed(33));
        assert_eq!(priority_color("nonsense"), DIM);
    }

    #[test]
    fn every_column_has_its_own_hue() {
        let cols = ["backlog", "in-progress", "blocked", "in-review", "done"];
        let set: std::collections::HashSet<_> = cols
            .iter()
            .map(|s| format!("{:?}", column_color(s)))
            .collect();
        assert_eq!(set.len(), cols.len(), "column colors must be distinct");
    }

    #[test]
    fn project_color_is_stable_and_spreads() {
        assert_eq!(project_color("TIDE"), project_color("TIDE"));
        let set: std::collections::HashSet<_> = ["FORGE", "TIDE", "PULSE", "CLI"]
            .iter()
            .map(|p| format!("{:?}", project_color(p)))
            .collect();
        assert!(set.len() >= 2, "demo projects should not all collide");
    }

    #[test]
    fn deadline_ramp_by_distance() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 28).unwrap();
        let c = |t: &str| deadline_color(Some(t), true, today);
        assert_eq!(c("2026-07-27"), ALARM, "overdue is alarm red");
        assert_eq!(c("2026-08-01"), Color::Indexed(208), "this week is orange");
        assert_eq!(c("2026-08-20"), Color::Indexed(226), "this month is yellow");
        assert_eq!(c("2026-12-01"), DIM, "far targets stay quiet");
        assert_eq!(c("not-a-date"), DIM, "garbage stays quiet");
        assert_eq!(
            deadline_color(Some("2026-07-01"), false, today),
            DIM,
            "completed/cancelled rows never glow"
        );
        assert_eq!(deadline_color(None, true, today), DIM);
    }

    #[test]
    fn hints_color_keys_and_dim_labels() {
        let line = hints(&[("q", "quit"), ("r", "refresh")]);
        assert_eq!(line.spans[0].content, "q");
        assert_eq!(line.spans[0].style.fg, Some(ACCENT));
        assert_eq!(line.spans[1].content, " quit");
        assert_eq!(line.spans[1].style.fg, Some(DIM));
        // Pairs are separated so the line reads `q quit  r refresh`.
        let flat: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(flat, "q quit  r refresh");
    }
}
