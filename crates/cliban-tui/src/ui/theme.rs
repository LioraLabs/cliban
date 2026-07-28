//! The cliban look, in one place.
//!
//! Every screen pulls its colors and chrome from here so the board, the
//! milestone page, and the popups read as one application instead of five
//! widgets that happen to share a binary. The palette sticks to 256-color
//! indexes (never truecolor) so it survives tmux, SSH sessions, and the
//! screenshot pipeline unchanged.
//!
//! ## Overrides
//!
//! Every named slot can be re-colored from `~/.config/cliban/theme.toml`
//! (`$XDG_CONFIG_HOME` respected, `CLIBAN_THEME_FILE` overrides outright) —
//! one `slot = color` per line, `#` comments. Colors parse the way ratatui
//! reads them: `yellow`, `light blue`, an index like `208`, or `#rrggbb`.
//! The dark-terminal defaults below apply to anything unset, so a light
//! theme only has to override what hurts. Slots:
//!
//! ```text
//! accent  dim  selection-bg  marker  alarm  milestone
//! priority-urgent  priority-high  priority-medium  priority-low
//! column-backlog  column-in-progress  column-blocked  column-in-review  column-done
//! status-open  status-completed  status-cancelled
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

static OVERRIDES: OnceLock<HashMap<String, Color>> = OnceLock::new();

/// Load palette overrides; call once at startup, before the first draw.
/// Missing file → defaults; a second call is a no-op.
pub fn load_overrides() {
    let path = match std::env::var("CLIBAN_THEME_FILE") {
        Ok(p) if !p.trim().is_empty() => std::path::PathBuf::from(p),
        _ => {
            let base = match std::env::var("XDG_CONFIG_HOME") {
                Ok(v) if !v.is_empty() => std::path::PathBuf::from(v),
                _ => cliban_core::paths::home_dir().join(".config"),
            };
            base.join("cliban").join("theme.toml")
        }
    };
    let map = std::fs::read_to_string(path)
        .map(|s| parse_theme(&s))
        .unwrap_or_default();
    let _ = OVERRIDES.set(map);
}

/// `slot = color` lines into a map; unknown colors and junk lines are
/// skipped rather than fatal — a typo'd slot falls back to the default.
/// `#` starts a comment, except that a value may itself be a `#rrggbb`
/// literal (quoted or bare), so comments are stripped value-side only.
fn parse_theme(src: &str) -> HashMap<String, Color> {
    let mut out = HashMap::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        let v = if let Some(r) = v.strip_prefix('"') {
            r.split('"').next().unwrap_or("")
        } else if let Some(r) = v.strip_prefix('\'') {
            r.split('\'').next().unwrap_or("")
        } else if v.starts_with('#') {
            // A bare hex literal; anything after whitespace is comment.
            v.split_whitespace().next().unwrap_or(v)
        } else {
            v.split('#').next().unwrap_or("").trim()
        };
        if let Ok(c) = v.trim().parse::<Color>() {
            out.insert(k.trim().to_string(), c);
        }
    }
    out
}

fn over(slot: &str) -> Option<Color> {
    OVERRIDES.get()?.get(slot).copied()
}

/// Focus and chrome accent — the one color that always means "you are here".
pub fn accent() -> Color {
    over("accent").unwrap_or(Color::Cyan)
}
/// De-emphasised text: hints, counts, metadata.
pub fn dim() -> Color {
    over("dim").unwrap_or(Color::DarkGray)
}
/// Background wash behind the selected row of any list.
pub fn selection_bg() -> Color {
    over("selection-bg").unwrap_or(Color::Indexed(237))
}
/// The cursor marker (`▸`) and input carets.
pub fn marker() -> Color {
    over("marker").unwrap_or(Color::Yellow)
}
/// Alarm red — overdue targets, blocked counts, open blockers.
pub fn alarm() -> Color {
    over("alarm").unwrap_or(Color::Indexed(196))
}
/// Milestone references (scope chip, detail field) — violet, everywhere.
pub fn milestone_accent() -> Color {
    over("milestone").unwrap_or(Color::Indexed(141))
}

/// Priority palette, shared by card borders, priority letters, and the
/// detail popup. Loud on purpose: priority is the one thing the board
/// should shout about.
pub fn priority_color(p: &str) -> Color {
    if let Some(c) = over(&format!("priority-{p}")) {
        return c;
    }
    match p {
        "urgent" => Color::Indexed(196),
        "high" => Color::Indexed(208),
        "medium" => Color::Indexed(226),
        "low" => Color::Indexed(33),
        _ => dim(),
    }
}

/// Column accents: each board column gets a hue so headers scan at a glance
/// — muted cousins of the priority palette rather than competitors to it.
pub fn column_color(status: &str) -> Color {
    if let Some(c) = over(&format!("column-{status}")) {
        return c;
    }
    match status {
        "backlog" => Color::Indexed(110),
        "in-progress" => Color::Indexed(81),
        "blocked" => Color::Indexed(203),
        "in-review" => Color::Indexed(141),
        "done" => Color::Indexed(78),
        _ => dim(),
    }
}

/// Milestone lifecycle colors: open is active-blue, completed is green,
/// cancelled fades to gray.
pub fn milestone_status_color(status: &str) -> Color {
    if let Some(c) = over(&format!("status-{status}")) {
        return c;
    }
    match status {
        "open" => Color::Indexed(81),
        "completed" => Color::Indexed(78),
        "cancelled" => Color::Indexed(245),
        _ => dim(),
    }
}

/// Stable per-string color, hashed so `TIDE` (or the label `bug`) is the
/// same color on every screen and every run. The palette deliberately
/// avoids the priority hues so a tag never reads as an urgency signal.
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
        return dim();
    }
    let Some(d) = target.and_then(|t| chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d").ok()) else {
        return dim();
    };
    let days = (d - today).num_days();
    if days < 0 {
        alarm()
    } else if days <= 7 {
        priority_color("high")
    } else if days <= 30 {
        priority_color("medium")
    } else {
        dim()
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
            Style::default().fg(accent()),
        ));
        spans.push(Span::styled(
            format!(" {label}"),
            Style::default().fg(dim()),
        ));
    }
    Line::from(spans)
}

/// The selected list row: marker, bold label, and a background wash padded
/// to the full width so the row reads as one bar.
pub fn selected_row(label: &str, width: usize) -> Line<'static> {
    let base = Style::default().bg(selection_bg());
    let mut spans = vec![
        Span::styled("▸ ", base.fg(marker())),
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

    // NOTE: none of these call `load_overrides` — the OnceLock is global to
    // the test process, and every other suite asserts the default palette.
    // The parser is tested pure instead.

    #[test]
    fn theme_file_parses_names_indexes_hex_and_comments() {
        let map = parse_theme(
            "# a light theme\n\
             accent = blue\n\
             selection-bg = \"#e0e0e0\"\n\
             dim = #a0a0a0   # bare hex with a trailing comment\n\
             priority-high = 130   # burnt orange\n\
             nonsense line\n\
             marker = not-a-color\n",
        );
        assert_eq!(map.get("accent"), Some(&Color::Blue));
        assert_eq!(map.get("selection-bg"), Some(&Color::Rgb(0xe0, 0xe0, 0xe0)));
        assert_eq!(map.get("dim"), Some(&Color::Rgb(0xa0, 0xa0, 0xa0)));
        assert_eq!(map.get("priority-high"), Some(&Color::Indexed(130)));
        assert!(!map.contains_key("marker"), "unparsable colors are skipped");
        assert_eq!(map.len(), 4);
    }

    #[test]
    fn priority_palette_matches_cliban() {
        assert_eq!(priority_color("urgent"), Color::Indexed(196));
        assert_eq!(priority_color("high"), Color::Indexed(208));
        assert_eq!(priority_color("medium"), Color::Indexed(226));
        assert_eq!(priority_color("low"), Color::Indexed(33));
        assert_eq!(priority_color("nonsense"), dim());
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
        assert_eq!(c("2026-07-27"), alarm(), "overdue is alarm red");
        assert_eq!(c("2026-08-01"), Color::Indexed(208), "this week is orange");
        assert_eq!(c("2026-08-20"), Color::Indexed(226), "this month is yellow");
        assert_eq!(c("2026-12-01"), dim(), "far targets stay quiet");
        assert_eq!(c("not-a-date"), dim(), "garbage stays quiet");
        assert_eq!(
            deadline_color(Some("2026-07-01"), false, today),
            dim(),
            "completed/cancelled rows never glow"
        );
        assert_eq!(deadline_color(None, true, today), dim());
    }

    #[test]
    fn hints_color_keys_and_dim_labels() {
        let line = hints(&[("q", "quit"), ("r", "refresh")]);
        assert_eq!(line.spans[0].content, "q");
        assert_eq!(line.spans[0].style.fg, Some(accent()));
        assert_eq!(line.spans[1].content, " quit");
        assert_eq!(line.spans[1].style.fg, Some(dim()));
        let flat: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(flat, "q quit  r refresh");
    }
}
