//! Terminal event loop + Command dispatch over the Data adapter.
//! Generic over ratatui `Backend` and the `Session` input source so the same
//! loop runs on the local tty (crossterm) or headless/SSH.
use crate::actions::{Action, Command};
use crate::app::{App, Mode, PickerChip, PickerState};
use crate::buffers::{parse_issue, parse_milestone, parse_project, IssueBuffer, MilestoneBuffer};
use crate::data::Data;
use crate::session::{LocalSession, Session, SessionEvent};
use crossterm::event::KeyEvent;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;
use std::io::stdout;
use std::path::Path;
use std::time::Duration;

type DynErr = Box<dyn std::error::Error>;

pub fn dispatch_command(data: &Data, _app: &mut App, cmd: &Command) -> Result<bool, DynErr> {
    match cmd {
        Command::MoveIssue { key, status } => {
            data.move_issue(key, status)?;
            Ok(true)
        }
        Command::Reorder { key, other } => {
            data.reorder(key, other)?;
            Ok(true)
        }
        Command::Archive { key } => {
            data.archive(key)?;
            Ok(true)
        }
        Command::TagMilestone { key, milestone } => {
            data.tag_milestone(key, milestone.clone())?;
            Ok(true)
        }
        Command::SetScope | Command::Reload => Ok(true),
        _ => Ok(false), // editor commands handled in the loop
    }
}

pub fn reload(data: &Data, app: &mut App) -> Result<(), DynErr> {
    app.cards = data.load_cards()?;
    app.milestones = data.load_milestones(app.scope.project.as_deref())?;
    app.auto_focus_if_empty();
    Ok(())
}

fn temp_path(stem: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cliban-{}-{}.md", stem, std::process::id()))
}

fn run_editor<B: Backend>(
    terminal: &mut Terminal<B>,
    session: &mut dyn Session,
    path: &Path,
) -> Result<bool, DynErr> {
    let ok = session.run_editor(path)?;
    terminal.clear()?; // repaint after the editor owned the screen
    Ok(ok)
}

fn handle_editor<B: Backend>(
    data: &Data,
    app: &mut App,
    terminal: &mut Terminal<B>,
    session: &mut dyn Session,
    cmd: &Command,
) -> Result<(), DynErr> {
    match cmd {
        Command::EditIssue { key } => {
            let path = temp_path(&format!("issue-{key}"));
            std::fs::write(&path, data.issue_buffer(key)?.serialize())?;
            if run_editor(terminal, session, &path)? {
                if let Ok(p) = parse_issue(&std::fs::read_to_string(&path)?) {
                    data.apply_issue_edit(key, &p)?;
                }
            }
            let _ = std::fs::remove_file(&path);
        }
        Command::NewIssue { status } => {
            let buf = IssueBuffer {
                status: status.clone(),
                priority: "none".into(),
                ..Default::default()
            };
            let path = temp_path("new-issue");
            std::fs::write(&path, buf.serialize())?;
            if run_editor(terminal, session, &path)? {
                if let Ok(p) = parse_issue(&std::fs::read_to_string(&path)?) {
                    let project = app
                        .scope
                        .project
                        .clone()
                        .or_else(|| app.focused_card().map(|c| c.project.clone()))
                        .or_else(|| app.cards.first().map(|c| c.project.clone()));
                    match project {
                        Some(pj) => data.create_issue(&pj, &p)?,
                        None => app.status_msg = Some("scope a project (p) before creating".into()),
                    }
                }
            }
            let _ = std::fs::remove_file(&path);
        }
        Command::EditMilestone { name } => {
            if let Some(project) = app.scope.project.clone() {
                let path = temp_path("milestone");
                std::fs::write(&path, data.milestone_buffer(&project, name)?.serialize())?;
                if run_editor(terminal, session, &path)? {
                    if let Ok(p) = parse_milestone(&std::fs::read_to_string(&path)?) {
                        data.apply_milestone_edit(&project, name, &p)?;
                    }
                }
                let _ = std::fs::remove_file(&path);
            }
        }
        Command::NewMilestone => match app.scope.project.clone() {
            Some(project) => {
                let buf = MilestoneBuffer {
                    status: "open".into(),
                    ..Default::default()
                };
                let path = temp_path("new-milestone");
                std::fs::write(&path, buf.serialize())?;
                if run_editor(terminal, session, &path)? {
                    if let Ok(p) = parse_milestone(&std::fs::read_to_string(&path)?) {
                        data.create_milestone(&project, &p)?;
                    }
                }
                let _ = std::fs::remove_file(&path);
            }
            None => app.status_msg = Some("scope a project (p) before adding a milestone".into()),
        },
        Command::EditProject => {
            if let Some(project) = app.scope.project.clone() {
                let path = temp_path("project");
                std::fs::write(&path, data.project_buffer(&project)?.serialize())?;
                if run_editor(terminal, session, &path)? {
                    if let Ok(p) = parse_project(&std::fs::read_to_string(&path)?) {
                        data.apply_project_edit(&project, &p)?;
                    }
                }
                let _ = std::fs::remove_file(&path);
            }
        }
        _ => {}
    }
    Ok(())
}

fn seed_project_picker(data: &Data, app: &mut App) -> Result<(), DynErr> {
    if let Mode::ProjectPicker(_) = &app.mode {
        let items = data
            .list_projects()?
            .into_iter()
            .map(|(k, n)| PickerChip {
                label: format!("{k}  {n}"),
                value: k,
            })
            .collect();
        app.mode = Mode::ProjectPicker(PickerState {
            query: String::new(),
            items,
            cursor: 0,
        });
    }
    Ok(())
}
fn seed_milestone_picker(app: &mut App) {
    if let Mode::MilestonePicker(_) = &app.mode {
        let items = app
            .milestones
            .iter()
            .map(|m| PickerChip {
                label: m.name.clone(),
                value: m.name.clone(),
            })
            .collect();
        app.mode = Mode::MilestonePicker(PickerState {
            query: String::new(),
            items,
            cursor: 0,
        });
    }
}

pub fn run(path: &Path) -> Result<(), DynErr> {
    let data = Data::open(path)?;
    let mut app = App::new();
    reload(&data, &mut app)?;
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let res = event_loop(&mut terminal, &mut LocalSession, &data, &mut app);
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    res
}

/// Draw/input loop over any backend + session. Public so future hosts
/// can drive a TUI without the local-tty setup in [`run`].
pub fn event_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    session: &mut dyn Session,
    data: &Data,
    app: &mut App,
) -> Result<(), DynErr> {
    loop {
        terminal.draw(|f| crate::ui::render(f, app))?;
        match session.next_event(Duration::from_millis(100))? {
            // Tick → redraw (marquee advances). Resize → redraw too: the
            // backend reports its new size on the next draw (crossterm
            // autoresizes; headless harnesses resize their TestBackend).
            SessionEvent::Tick | SessionEvent::Resize(..) => continue,
            // Another writer changed the store: coarse re-query; the top of
            // the loop redraws. (Reload failure ends the session — correct
            // for e.g. a tenant deleted out from under it.)
            SessionEvent::Refresh => reload(data, app)?,
            SessionEvent::Key(key) => {
                if handle_key(terminal, session, data, app, key)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Process one key press. Returns true when the app should quit.
fn handle_key<B: Backend>(
    terminal: &mut Terminal<B>,
    session: &mut dyn Session,
    data: &Data,
    app: &mut App,
    key: KeyEvent,
) -> Result<bool, DynErr> {
    let Some(action) = crate::keybinds::map_key(key, app) else {
        return Ok(false);
    };
    if matches!(action, Action::Quit) {
        return Ok(true);
    }
    let open_pp = matches!(action, Action::OpenProjectPicker);
    let open_mp = matches!(action, Action::OpenMilestonePicker);
    let cmd = crate::app::update(app, action);
    if open_pp {
        seed_project_picker(data, app)?;
    }
    if open_mp {
        seed_milestone_picker(app);
    }
    if let Some(cmd) = cmd {
        match cmd {
            Command::EditIssue { .. }
            | Command::NewIssue { .. }
            | Command::EditMilestone { .. }
            | Command::NewMilestone
            | Command::EditProject => {
                handle_editor(data, app, terminal, session, &cmd)?;
                reload(data, app)?;
            }
            other => {
                if dispatch_command(data, app, &other)? {
                    reload(data, app)?;
                }
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::app::Mode;
    use crate::input::ByteSession;
    use ratatui::backend::TestBackend;

    fn dump(t: &Terminal<TestBackend>) -> String {
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

    /// Drain every queued event through the real key handler, then draw one
    /// frame — the "bytes in, frames out" pump.
    fn pump(t: &mut Terminal<TestBackend>, s: &mut ByteSession, data: &Data, app: &mut App) {
        loop {
            match s.next_event(Duration::ZERO).unwrap() {
                SessionEvent::Tick => break,
                SessionEvent::Resize(w, h) => t.backend_mut().resize(w, h),
                SessionEvent::Refresh => reload(data, app).unwrap(),
                SessionEvent::Key(k) => {
                    if handle_key(t, s, data, app, k).unwrap() {
                        break;
                    }
                }
            }
        }
        t.draw(|f| crate::ui::render(f, app)).unwrap();
    }

    fn harness() -> (Data, App, Terminal<TestBackend>, ByteSession) {
        let data = Data::open_in_memory_for_test();
        data.seed_project_issue("CLI", "First");
        let mut app = App::new();
        reload(&data, &mut app).unwrap();
        let terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        (data, app, terminal, ByteSession::new())
    }

    #[test]
    fn headless_bytes_move_card_to_done_column() {
        let (data, mut app, mut t, mut s) = harness();
        s.feed_bytes(b" d"); // Space begins move, 'd' -> done
        pump(&mut t, &mut s, &data, &mut app);
        assert_eq!(app.cards[0].status, "done");
        let d = dump(&t);
        assert!(
            d.contains("DONE (1)"),
            "frame should show the card in DONE:\n{d}"
        );
    }

    #[test]
    fn headless_help_overlay_renders_in_frame() {
        let (data, mut app, mut t, mut s) = harness();
        s.feed_bytes(b"?");
        pump(&mut t, &mut s, &data, &mut app);
        let d = dump(&t);
        assert!(
            d.contains("Help · cliban"),
            "frame should show the help popup:\n{d}"
        );
    }

    #[test]
    fn headless_csi_down_arrow_moves_focus() {
        let (data, mut app, mut t, mut s) = harness();
        data.seed_issue("CLI", "Second");
        reload(&data, &mut app).unwrap();
        assert_eq!(app.focus.card_idx, 0);
        s.feed_bytes(b"\x1b[B"); // CSI down arrow, as an SSH client would send
        pump(&mut t, &mut s, &data, &mut app);
        assert_eq!(app.focus.card_idx, 1);
    }

    #[test]
    fn headless_esc_byte_opens_confirm_quit() {
        let (data, mut app, mut t, mut s) = harness();
        s.feed_bytes(b"\x1b"); // lone ESC: delivered via the idle flush path
        pump(&mut t, &mut s, &data, &mut app);
        assert!(matches!(app.mode, Mode::ConfirmQuit));
    }

    #[test]
    fn headless_resize_injection_redraws_at_new_size() {
        let (data, mut app, mut t, mut s) = harness();
        s.inject_resize(60, 12);
        pump(&mut t, &mut s, &data, &mut app);
        let area = t.backend().buffer().area;
        assert_eq!((area.width, area.height), (60, 12));
        assert!(
            dump(&t).contains("BACKLOG"),
            "board should render at the new size"
        );
    }

    #[test]
    fn full_event_loop_runs_headless_and_quits() {
        let (data, mut app, mut t, mut s) = harness();
        s.feed_bytes(b"qy"); // q -> confirm-quit prompt, y -> quit
        event_loop(&mut t, &mut s, &data, &mut app).unwrap();
        // Reaching here proves the real loop ran and terminated headless.
        assert!(dump(&t).contains("BACKLOG (1)"));
    }

    #[test]
    fn move_command_changes_status_and_reload_reflects_it() {
        let data = Data::open_in_memory_for_test();
        data.seed_project_issue("CLI", "First");
        let mut app = App::new();
        reload(&data, &mut app).unwrap();
        let cmd = Command::MoveIssue {
            key: "CLI-1".into(),
            status: "done".into(),
        };
        assert!(dispatch_command(&data, &mut app, &cmd).unwrap());
        reload(&data, &mut app).unwrap();
        assert_eq!(app.cards[0].status, "done");
    }

    #[test]
    fn refresh_event_pulls_in_external_changes_without_input() {
        let (data, mut app, mut t, mut s) = harness();
        let key = app.cards[0].key.clone();
        // Another session over the same shared store moves the card.
        let other = Data::from_store(data.store.clone()).unwrap();
        other.move_issue(&key, "done").unwrap();
        assert_eq!(app.cards[0].status, "backlog"); // stale until refreshed
        s.inject_refresh();
        pump(&mut t, &mut s, &data, &mut app);
        assert_eq!(app.cards[0].status, "done");
        let d = dump(&t);
        assert!(d.contains("DONE (1)"), "frame shows the remote move:\n{d}");
    }

    #[test]
    fn archive_command_then_reload_empties_board() {
        let data = Data::open_in_memory_for_test();
        data.seed_project_issue("CLI", "First");
        let mut app = App::new();
        reload(&data, &mut app).unwrap();
        assert!(dispatch_command(
            &data,
            &mut app,
            &Command::Archive {
                key: "CLI-1".into()
            }
        )
        .unwrap());
        reload(&data, &mut app).unwrap();
        assert!(app.cards.is_empty());
    }
}
