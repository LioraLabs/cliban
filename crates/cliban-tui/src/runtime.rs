//! Terminal event loop + Command dispatch over the Data adapter.
use std::io::stdout;
use std::path::Path;
use std::time::Duration;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use crate::actions::{Action, Command};
use crate::app::{App, Mode, PickerChip, PickerState};
use crate::buffers::{parse_issue, parse_milestone, parse_project, IssueBuffer, MilestoneBuffer};
use crate::data::Data;

type Term = Terminal<CrosstermBackend<std::io::Stdout>>;
type DynErr = Box<dyn std::error::Error>;

pub fn dispatch_command(data: &Data, _app: &mut App, cmd: &Command) -> Result<bool, DynErr> {
    match cmd {
        Command::MoveIssue { key, status } => { data.move_issue(key, status)?; Ok(true) }
        Command::Reorder { key, other } => { data.reorder(key, other)?; Ok(true) }
        Command::Archive { key } => { data.archive(key)?; Ok(true) }
        Command::TagMilestone { key, milestone } => { data.tag_milestone(key, milestone.clone())?; Ok(true) }
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

fn resolve_editor() -> String { std::env::var("EDITOR").unwrap_or_else(|_| "vi".into()) }
fn temp_path(stem: &str) -> std::path::PathBuf { std::env::temp_dir().join(format!("cliban-{}-{}.md", stem, std::process::id())) }

fn run_editor(terminal: &mut Term, path: &Path) -> Result<bool, DynErr> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    let status = std::process::Command::new("sh").arg("-c")
        .arg(format!("{} {:?}", resolve_editor(), path)).status();
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    terminal.clear()?;
    Ok(matches!(status, Ok(s) if s.success()))
}

fn handle_editor(data: &Data, app: &mut App, terminal: &mut Term, cmd: &Command) -> Result<(), DynErr> {
    match cmd {
        Command::EditIssue { key } => {
            let path = temp_path(&format!("issue-{key}"));
            std::fs::write(&path, data.issue_buffer(key)?.serialize())?;
            if run_editor(terminal, &path)? {
                if let Ok(p) = parse_issue(&std::fs::read_to_string(&path)?) { data.apply_issue_edit(key, &p)?; }
            }
            let _ = std::fs::remove_file(&path);
        }
        Command::NewIssue { status } => {
            let buf = IssueBuffer { status: status.clone(), priority: "none".into(), ..Default::default() };
            let path = temp_path("new-issue");
            std::fs::write(&path, buf.serialize())?;
            if run_editor(terminal, &path)? {
                if let Ok(p) = parse_issue(&std::fs::read_to_string(&path)?) {
                    let project = app.scope.project.clone()
                        .or_else(|| app.focused_card().map(|c| c.project.clone()))
                        .or_else(|| app.cards.first().map(|c| c.project.clone()));
                    match project { Some(pj) => data.create_issue(&pj, &p)?, None => app.status_msg = Some("scope a project (p) before creating".into()) }
                }
            }
            let _ = std::fs::remove_file(&path);
        }
        Command::EditMilestone { name } => {
            if let Some(project) = app.scope.project.clone() {
                let path = temp_path("milestone");
                std::fs::write(&path, data.milestone_buffer(&project, name)?.serialize())?;
                if run_editor(terminal, &path)? {
                    if let Ok(p) = parse_milestone(&std::fs::read_to_string(&path)?) { data.apply_milestone_edit(&project, name, &p)?; }
                }
                let _ = std::fs::remove_file(&path);
            }
        }
        Command::NewMilestone => {
            match app.scope.project.clone() {
                Some(project) => {
                    let buf = MilestoneBuffer { status: "open".into(), ..Default::default() };
                    let path = temp_path("new-milestone");
                    std::fs::write(&path, buf.serialize())?;
                    if run_editor(terminal, &path)? {
                        if let Ok(p) = parse_milestone(&std::fs::read_to_string(&path)?) { data.create_milestone(&project, &p)?; }
                    }
                    let _ = std::fs::remove_file(&path);
                }
                None => app.status_msg = Some("scope a project (p) before adding a milestone".into()),
            }
        }
        Command::EditProject => {
            if let Some(project) = app.scope.project.clone() {
                let path = temp_path("project");
                std::fs::write(&path, data.project_buffer(&project)?.serialize())?;
                if run_editor(terminal, &path)? {
                    if let Ok(p) = parse_project(&std::fs::read_to_string(&path)?) { data.apply_project_edit(&project, &p)?; }
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
        let items = data.list_projects()?.into_iter().map(|(k, n)| PickerChip { label: format!("{k}  {n}"), value: k }).collect();
        app.mode = Mode::ProjectPicker(PickerState { query: String::new(), items, cursor: 0 });
    }
    Ok(())
}
fn seed_milestone_picker(app: &mut App) {
    if let Mode::MilestonePicker(_) = &app.mode {
        let items = app.milestones.iter().map(|m| PickerChip { label: m.name.clone(), value: m.name.clone() }).collect();
        app.mode = Mode::MilestonePicker(PickerState { query: String::new(), items, cursor: 0 });
    }
}

pub fn run(path: &Path) -> Result<(), DynErr> {
    let data = Data::open(path)?;
    let mut app = App::new();
    reload(&data, &mut app)?;
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let res = event_loop(&mut terminal, &data, &mut app);
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    res
}

fn event_loop(terminal: &mut Term, data: &Data, app: &mut App) -> Result<(), DynErr> {
    loop {
        terminal.draw(|f| crate::ui::render(f, app))?;
        if !event::poll(Duration::from_millis(100))? { continue; } // tick → marquee advances
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press { continue; }
            let Some(action) = crate::keybinds::map_key(key, app) else { continue; };
            if matches!(action, Action::Quit) { break; }
            let open_pp = matches!(action, Action::OpenProjectPicker);
            let open_mp = matches!(action, Action::OpenMilestonePicker);
            let cmd = crate::app::update(app, action);
            if open_pp { seed_project_picker(data, app)?; }
            if open_mp { seed_milestone_picker(app); }
            if let Some(cmd) = cmd {
                match cmd {
                    Command::EditIssue{..} | Command::NewIssue{..} | Command::EditMilestone{..}
                    | Command::NewMilestone | Command::EditProject => { handle_editor(data, app, terminal, &cmd)?; reload(data, app)?; }
                    other => { if dispatch_command(data, app, &other)? { reload(data, app)?; } }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    #[test]
    fn move_command_changes_status_and_reload_reflects_it() {
        let data = Data::open_in_memory_for_test();
        data.seed_project_issue("CLI", "First");
        let mut app = App::new();
        reload(&data, &mut app).unwrap();
        let cmd = Command::MoveIssue { key: "CLI-1".into(), status: "done".into() };
        assert!(dispatch_command(&data, &mut app, &cmd).unwrap());
        reload(&data, &mut app).unwrap();
        assert_eq!(app.cards[0].status, "done");
    }

    #[test]
    fn archive_command_then_reload_empties_board() {
        let data = Data::open_in_memory_for_test();
        data.seed_project_issue("CLI", "First");
        let mut app = App::new();
        reload(&data, &mut app).unwrap();
        assert!(dispatch_command(&data, &mut app, &Command::Archive { key: "CLI-1".into() }).unwrap());
        reload(&data, &mut app).unwrap();
        assert!(app.cards.is_empty());
    }
}
