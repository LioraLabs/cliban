//! Session I/O abstraction: input events + editor suspension, decoupled from
//! the local tty so the TUI can later run over an SSH channel.
//!
//! Output goes through ratatui's `Backend` trait (crossterm's backend accepts
//! any `impl Write`) and terminal size comes from that same backend, so the
//! only things a session must provide are input events and `$EDITOR`
//! handling. crossterm's `KeyEvent` types are plain data (not tty-bound) and
//! remain the app-wide key vocabulary.

use std::io;
use std::path::Path;
use std::time::Duration;

use crossterm::event::KeyEvent;

/// One app-level input event delivered to the event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    Key(KeyEvent),
    /// Terminal resized to (cols, rows). The loop just redraws; the backend
    /// is responsible for reporting its new size on the next draw.
    Resize(u16, u16),
    /// The board's underlying data changed outside this session (another
    /// writer on the same store): re-query, then redraw.
    Refresh,
    /// Nothing happened within the poll timeout (or an ignorable event
    /// arrived). Drives marquee animation via redraw.
    Tick,
}

/// Where input events come from and how the UI is suspended for `$EDITOR`.
pub trait Session {
    /// Block up to `timeout` for the next event; `Tick` on timeout.
    fn next_event(&mut self, timeout: Duration) -> io::Result<SessionEvent>;

    /// Suspend the UI, run `$EDITOR` on `path`, resume. Returns true if the
    /// editor exited successfully. Sessions without a local tty can't spawn
    /// an editor; the default declines the edit.
    fn run_editor(&mut self, path: &Path) -> io::Result<bool> {
        let _ = path;
        Ok(false)
    }
}

/// Local-tty session: crossterm events + the raw-mode/alt-screen editor
/// dance. Wires the abstraction to crossterm exactly as before the split.
pub struct LocalSession;

impl Session for LocalSession {
    fn next_event(&mut self, timeout: Duration) -> io::Result<SessionEvent> {
        use crossterm::event::{self, Event, KeyEventKind};
        if !event::poll(timeout)? {
            return Ok(SessionEvent::Tick);
        }
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => Ok(SessionEvent::Key(k)),
            Event::Resize(w, h) => Ok(SessionEvent::Resize(w, h)),
            _ => Ok(SessionEvent::Tick),
        }
    }

    fn run_editor(&mut self, path: &Path) -> io::Result<bool> {
        use crossterm::execute;
        use crossterm::terminal::{
            disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
        };
        disable_raw_mode()?;
        execute!(std::io::stdout(), LeaveAlternateScreen)?;
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{} {:?}", resolve_editor(), path))
            .status();
        enable_raw_mode()?;
        execute!(std::io::stdout(), EnterAlternateScreen)?;
        Ok(matches!(status, Ok(s) if s.success()))
    }
}

fn resolve_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "vi".into())
}
