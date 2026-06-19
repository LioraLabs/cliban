use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// Categorised keybind sections for the cliban board.
const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    ("NAVIGATION", &[("h j k l","move cursor"), ("gg / G","top / bottom of column"), ("Tab","cycle column")]),
    ("CARD", &[("Enter","open detail"), ("e","edit issue ($EDITOR)"), ("Space b/i/k/r/d","move to status"),
               ("t","cycle milestone tag"), ("a","archive"), ("n","new issue")]),
    ("SCOPE / MILESTONES", &[("p","project picker"), ("m","milestone overlay"), ("M","cycle milestone filter"),
               ("N","new milestone"), ("E","edit milestone/project ($EDITOR)"), ("/","fuzzy find")]),
    ("VIEW", &[("r","refresh"), ("?","this help"), ("q","quit"), ("Esc","close popup / quit")]),
];

const FOOTER: &str = "cliban — loom-style board";

const KEY_COL_WIDTH: usize = 16;

pub fn draw_help(frame: &mut Frame, area: Rect) {
    let lines = build_lines();
    // Width fits the longest "  key   description" line plus borders/padding.
    // Height = blank + (header + rows + blank) per section + footer rows.
    let height = lines.len() as u16 + 2; // +2 for top/bottom border
    let popup = centered_rect(56, height, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Help · cliban ")
        .borders(Borders::ALL);
    let p = Paragraph::new(lines).block(block);
    frame.render_widget(p, popup);
}

fn build_lines() -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::raw(""));

    for (i, (header, rows)) in SECTIONS.iter().enumerate() {
        if i > 0 {
            lines.push(Line::raw(""));
        }
        lines.push(Line::from(Span::styled(
            format!(" {}", header),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in rows.iter() {
            let padded_key = format!("{:<width$}", key, width = KEY_COL_WIDTH);
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(padded_key, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(*desc),
            ]));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        format!(" {}", FOOTER),
        Style::default().add_modifier(Modifier::DIM),
    )));
    lines.push(Line::raw(""));
    lines
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let h = height.min(area.height);
    let w = width.min(area.width);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render() -> String {
        let backend = TestBackend::new(80, 50);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_help(f, Rect::new(0, 0, 80, 50)))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut dump = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                dump.push_str(buf[(x, y)].symbol());
            }
            dump.push('\n');
        }
        dump
    }

    #[test]
    fn help_overlay_renders_title() {
        let dump = render();
        assert!(dump.contains("Help"), "title missing:\n{dump}");
        assert!(dump.contains("cliban"), "subtitle missing:\n{dump}");
    }

    #[test]
    fn help_lists_cliban_keys() {
        let dump = render();
        for needle in [
            "move cursor",
            "edit issue",
            "move to status",
            "cycle milestone tag",
            "archive",
            "new milestone",
            "fuzzy find",
            "milestone overlay",
        ] {
            assert!(dump.contains(needle), "missing `{needle}`:\n{dump}");
        }
    }
}
