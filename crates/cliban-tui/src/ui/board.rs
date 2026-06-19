use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;
use crate::app::{App, ColumnId};

pub fn draw_board(frame: &mut Frame, area: Rect, app: &App) {
    let columns = app.visible_columns();
    let n = columns.len() as u32;
    let constraints: Vec<Constraint> = columns.iter().map(|_| Constraint::Ratio(1, n)).collect();
    let col_areas = Layout::default().direction(Direction::Horizontal).constraints(constraints).split(area);
    let now_ms = app.boot_at.elapsed().as_millis();
    for (i, col) in columns.iter().enumerate() { draw_column(frame, col_areas[i], app, *col, now_ms); }
}

fn draw_column(frame: &mut Frame, area: Rect, app: &App, col: ColumnId, now_ms: u128) {
    let cards = app.column_cards(col);
    let header = format!(" {} ({}) ", col.label(), cards.len());
    let block = Block::default().title(header).borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if cards.is_empty() { return; }
    let card_height: u16 = 4;
    let max_visible = (inner.height / card_height) as usize;
    if max_visible == 0 { return; }
    let scroll_start = if app.focus.column == col {
        let f = app.focus.card_idx;
        if f >= max_visible { f.saturating_sub(max_visible - 1) } else { 0 }
    } else { 0 };
    let mut y = inner.y;
    for (idx, card) in cards.iter().enumerate().skip(scroll_start) {
        if y + card_height > inner.y + inner.height { break; }
        let is_focused = app.focus.column == col && app.focus.card_idx == idx;
        super::card::draw_card(frame, Rect::new(inner.x, y, inner.width, card_height), card, is_focused, now_ms);
        y += card_height;
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{App, Card};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use ratatui::layout::Rect;

    fn dump(buf: &ratatui::buffer::Buffer) -> String {
        let mut s = String::new();
        for y in 0..buf.area.height { for x in 0..buf.area.width { s.push_str(buf[(x,y)].symbol()); } s.push('\n'); }
        s
    }
    fn card(key: &str, status: &str) -> Card {
        Card { id:0, key:key.into(), project:"CLI".into(), title:"x".into(), status:status.into(),
               priority:"low".into(), position:1.0, milestone_id:None, milestone:None }
    }

    #[test]
    fn board_renders_all_five_columns() {
        let mut t = Terminal::new(TestBackend::new(160,24)).unwrap();
        let app = App::new();
        t.draw(|f| super::draw_board(f, Rect::new(0,0,160,24), &app)).unwrap();
        let d = dump(t.backend().buffer());
        for l in ["BACKLOG","IN-PROGRESS","BLOCKED","IN-REVIEW","DONE"] { assert!(d.contains(l), "missing {l}:\n{d}"); }
    }

    #[test]
    fn column_header_shows_count() {
        let mut t = Terminal::new(TestBackend::new(160,24)).unwrap();
        let mut app = App::new();
        app.cards = vec![card("CLI-1","backlog"), card("CLI-2","backlog")];
        t.draw(|f| super::draw_board(f, Rect::new(0,0,160,24), &app)).unwrap();
        assert!(dump(t.backend().buffer()).contains("BACKLOG (2)"));
    }
}
