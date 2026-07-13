//! Hosting the TUI on a remote byte channel (SSH): a blocking-mpsc `Session`
//! plus a ratatui backend that renders to any `Write` with an
//! externally-controlled size (the local tty must never be consulted).

use std::collections::VecDeque;
use std::io;
use std::io::Write;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, Show};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};

use crate::input::Parser;
use crate::session::{Session, SessionEvent};

/// Input as the transport delivers it: raw terminal bytes plus out-of-band
/// resizes (SSH window-change requests).
#[derive(Debug)]
pub enum RemoteInput {
    Bytes(Vec<u8>),
    Resize(u16, u16),
}

/// Shared (cols, rows) cell: `ChannelSession` writes it when a resize
/// arrives; `RemoteBackend::size()` reads it on the next draw.
pub type SharedSize = Arc<Mutex<(u16, u16)>>;

/// Blocking `Session` over an mpsc of [`RemoteInput`]. The transport side
/// (SSH handler) only ever sends on the channel; dropping the sender ends
/// the session with an io error, which ends the event loop.
pub struct ChannelSession {
    rx: Receiver<RemoteInput>,
    parser: Parser,
    queue: VecDeque<SessionEvent>,
    size: SharedSize,
    /// Host-supplied "did the data change since the last call?" probe,
    /// polled once per event wait and once per poll timeout. `true` yields
    /// [`SessionEvent::Refresh`]. See [`ChannelSession::set_dirty_check`].
    dirty: Option<Box<dyn FnMut() -> bool + Send>>,
}

impl ChannelSession {
    pub fn new(rx: Receiver<RemoteInput>, size: SharedSize) -> Self {
        Self {
            rx,
            parser: Parser::new(),
            queue: VecDeque::new(),
            size,
            dirty: None,
        }
    }

    /// Attach a data-change probe (e.g. a drained broadcast receiver for
    /// this board's tenant). Polling keeps the transport untouched: no
    /// extra tasks, and dropping the input sender still ends the session.
    pub fn set_dirty_check(&mut self, f: impl FnMut() -> bool + Send + 'static) {
        self.dirty = Some(Box::new(f));
    }

    fn is_dirty(&mut self) -> bool {
        self.dirty.as_mut().is_some_and(|f| f())
    }
}

impl Session for ChannelSession {
    fn next_event(&mut self, timeout: Duration) -> io::Result<SessionEvent> {
        // Freshness first: a pending change beats queued keys (none are
        // dropped — they return on the very next call).
        if self.is_dirty() {
            return Ok(SessionEvent::Refresh);
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(ev) = self.queue.pop_front() {
                return Ok(ev);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(remaining) {
                Ok(RemoteInput::Bytes(b)) => {
                    for k in self.parser.feed(&b) {
                        self.queue.push_back(SessionEvent::Key(k));
                    }
                    // A partial escape sequence may queue nothing: loop.
                }
                Ok(RemoteInput::Resize(w, h)) => {
                    let (w, h) = (w.max(1), h.max(1));
                    *self
                        .size
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = (w, h);
                    return Ok(SessionEvent::Resize(w, h));
                }
                // Idle: a buffered lone ESC is a real Esc key press.
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(k) = self.parser.flush() {
                        return Ok(SessionEvent::Key(k));
                    }
                    if self.is_dirty() {
                        return Ok(SessionEvent::Refresh);
                    }
                    return Ok(SessionEvent::Tick);
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "remote input channel closed",
                    ));
                }
            }
        }
    }
    // run_editor: default declines — no local tty on the server side.
}

/// Ratatui backend for a remote terminal: ANSI rendering via
/// `CrosstermBackend<W>` over any writer, but size comes from the shared
/// cell (fed by pty-req/window-change) instead of the local tty, and cursor
/// position is never queried (there is no tty to ask).
pub struct RemoteBackend<W: Write> {
    inner: CrosstermBackend<W>,
    size: SharedSize,
}

impl<W: Write> RemoteBackend<W> {
    /// Wrap `writer` at the client's pty size, entering the alternate screen
    /// and hiding the cursor (flushed immediately). Returns the backend and
    /// the size cell to hand to [`ChannelSession`].
    pub fn new(writer: W, cols: u16, rows: u16) -> io::Result<(Self, SharedSize)> {
        let size: SharedSize = Arc::new(Mutex::new((cols.max(1), rows.max(1))));
        let mut inner = CrosstermBackend::new(writer);
        crossterm::queue!(inner.writer_mut(), EnterAlternateScreen, Hide)?;
        inner.writer_mut().flush()?;
        Ok((
            Self {
                inner,
                size: size.clone(),
            },
            size,
        ))
    }

    /// Best-effort client restore: leave the alternate screen, show the
    /// cursor. Call before hanging up; harmless if the peer is gone.
    pub fn leave_screen(&mut self) -> io::Result<()> {
        crossterm::queue!(self.inner.writer_mut(), LeaveAlternateScreen, Show)?;
        self.inner.writer_mut().flush()
    }
}

impl<W: Write> Backend for RemoteBackend<W> {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        // Would require a tty round-trip; unused by the fullscreen viewport.
        Ok(Position::ORIGIN)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        let (w, h) = *self
            .size
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(Size {
            width: w,
            height: h,
        })
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        let (w, h) = *self
            .size
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(WindowSize {
            columns_rows: Size {
                width: w,
                height: h,
            },
            pixels: Size {
                width: 0,
                height: 0,
            },
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::mpsc;

    fn session() -> (mpsc::Sender<RemoteInput>, ChannelSession, SharedSize) {
        let (tx, rx) = mpsc::channel();
        let size: SharedSize = Arc::new(Mutex::new((80, 24)));
        (tx, ChannelSession::new(rx, size.clone()), size)
    }

    fn key(code: KeyCode) -> SessionEvent {
        SessionEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn bytes_parse_to_key_events_in_order() {
        let (tx, mut s, _) = session();
        tx.send(RemoteInput::Bytes(b"j\x1b[B".to_vec())).unwrap();
        assert_eq!(
            s.next_event(Duration::ZERO).unwrap(),
            key(KeyCode::Char('j'))
        );
        assert_eq!(s.next_event(Duration::ZERO).unwrap(), key(KeyCode::Down));
    }

    #[test]
    fn resize_updates_shared_size_and_emits_event() {
        let (tx, mut s, size) = session();
        tx.send(RemoteInput::Resize(120, 40)).unwrap();
        assert_eq!(
            s.next_event(Duration::ZERO).unwrap(),
            SessionEvent::Resize(120, 40)
        );
        assert_eq!(*size.lock().unwrap(), (120, 40));
    }

    #[test]
    fn resize_clamps_zero_dimensions_to_one() {
        let (tx, mut s, size) = session();
        tx.send(RemoteInput::Resize(0, 0)).unwrap();
        assert_eq!(
            s.next_event(Duration::ZERO).unwrap(),
            SessionEvent::Resize(1, 1)
        );
        assert_eq!(*size.lock().unwrap(), (1, 1));
    }

    #[test]
    fn next_event_honors_a_deadline_despite_a_drip_of_zero_event_bytes() {
        let (tx, mut s, _) = session();
        // Prime the parser with a never-completing partial CSI sequence.
        tx.send(RemoteInput::Bytes(b"\x1b[".to_vec())).unwrap();
        // Then drip a byte every 10ms that keeps parsing to nothing (it just
        // extends the same pending CSI sequence), which would restart a
        // naive `recv_timeout(timeout)` loop forever.
        std::thread::spawn(move || {
            for _ in 0..40 {
                std::thread::sleep(Duration::from_millis(10));
                if tx.send(RemoteInput::Bytes(b"1".to_vec())).is_err() {
                    return;
                }
            }
        });
        let start = std::time::Instant::now();
        let result = s.next_event(Duration::from_millis(50));
        let elapsed = start.elapsed();
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert!(
            elapsed < Duration::from_millis(300),
            "next_event should honor the original deadline, took {elapsed:?}"
        );
    }

    #[test]
    fn timeout_yields_tick() {
        let (_tx, mut s, _) = session();
        assert_eq!(
            s.next_event(Duration::from_millis(1)).unwrap(),
            SessionEvent::Tick
        );
    }

    #[test]
    fn lone_esc_flushes_on_timeout_not_before() {
        let (tx, mut s, _) = session();
        tx.send(RemoteInput::Bytes(b"\x1b".to_vec())).unwrap();
        // First recv gets the bytes; nothing complete parses, then the
        // recv timeout expires and the pending ESC flushes as a key.
        assert_eq!(
            s.next_event(Duration::from_millis(1)).unwrap(),
            key(KeyCode::Esc)
        );
    }

    #[test]
    fn split_escape_sequence_across_sends_stays_one_key() {
        let (tx, mut s, _) = session();
        tx.send(RemoteInput::Bytes(b"\x1b[".to_vec())).unwrap();
        tx.send(RemoteInput::Bytes(b"B".to_vec())).unwrap();
        assert_eq!(
            s.next_event(Duration::from_millis(1)).unwrap(),
            key(KeyCode::Down)
        );
    }

    #[test]
    fn dropped_sender_is_an_error() {
        let (tx, mut s, _) = session();
        drop(tx);
        assert!(s.next_event(Duration::ZERO).is_err());
    }

    #[test]
    fn dirty_check_yields_refresh_then_goes_quiet() {
        let (_tx, mut s, _) = session();
        let fired = Arc::new(Mutex::new(true)); // true exactly once
        let f = fired.clone();
        s.set_dirty_check(move || std::mem::take(&mut *f.lock().unwrap()));
        assert_eq!(s.next_event(Duration::ZERO).unwrap(), SessionEvent::Refresh);
        // Consumed: the next wait times out to a plain Tick.
        assert_eq!(
            s.next_event(Duration::from_millis(1)).unwrap(),
            SessionEvent::Tick
        );
    }

    #[test]
    fn refresh_precedes_queued_keys_without_dropping_them() {
        let (tx, mut s, _) = session();
        tx.send(RemoteInput::Bytes(b"j".to_vec())).unwrap();
        let fired = Arc::new(Mutex::new(true));
        let f = fired.clone();
        s.set_dirty_check(move || std::mem::take(&mut *f.lock().unwrap()));
        assert_eq!(s.next_event(Duration::ZERO).unwrap(), SessionEvent::Refresh);
        assert_eq!(
            s.next_event(Duration::ZERO).unwrap(),
            key(KeyCode::Char('j'))
        );
    }

    #[test]
    fn broadcast_backed_dirty_check_coalesces_even_a_lagged_burst() {
        let (_tx, mut s, _) = session();
        // Deliberately small capacity: the burst below overflows it, so the
        // first try_recv yields Lagged — the drain must treat that as dirty.
        let (btx, mut brx) = tokio::sync::broadcast::channel::<()>(4);
        s.set_dirty_check(move || {
            let mut dirty = false;
            loop {
                match brx.try_recv() {
                    Ok(()) => dirty = true,
                    // Lagged means "you missed some": still just a refresh.
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => dirty = true,
                    Err(_) => break, // Empty or Closed: nothing pending
                }
            }
            dirty
        });
        for _ in 0..10 {
            btx.send(()).unwrap();
        }
        // Ten writes (some lost to lag), one refresh — the debounce the spec
        // asks for, robust to channel overflow.
        assert_eq!(s.next_event(Duration::ZERO).unwrap(), SessionEvent::Refresh);
        assert_eq!(
            s.next_event(Duration::from_millis(1)).unwrap(),
            SessionEvent::Tick
        );
    }

    /// Clonable in-memory writer so tests can inspect what the backend wrote
    /// after handing ownership to the backend/terminal.
    #[derive(Clone, Default)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl SharedBuf {
        fn contents(&self) -> Vec<u8> {
            self.0.lock().unwrap().clone()
        }
    }

    #[test]
    fn remote_backend_reports_injected_size_not_local_tty() {
        use ratatui::backend::Backend as _;
        let (backend, size) = RemoteBackend::new(SharedBuf::default(), 91, 33).unwrap();
        assert_eq!(backend.size().unwrap().width, 91);
        assert_eq!(backend.size().unwrap().height, 33);
        *size.lock().unwrap() = (120, 40);
        assert_eq!(backend.size().unwrap().width, 120);
        assert_eq!(backend.size().unwrap().height, 40);
    }

    #[test]
    fn new_enters_alt_screen_and_leave_restores() {
        let buf = SharedBuf::default();
        let (mut backend, _) = RemoteBackend::new(buf.clone(), 80, 24).unwrap();
        let s = String::from_utf8(buf.contents()).unwrap();
        assert!(s.contains("\x1b[?1049h"), "enter alt screen: {s:?}");
        assert!(s.contains("\x1b[?25l"), "hide cursor: {s:?}");
        backend.leave_screen().unwrap();
        let s = String::from_utf8(buf.contents()).unwrap();
        assert!(s.contains("\x1b[?1049l"), "leave alt screen: {s:?}");
        assert!(s.contains("\x1b[?25h"), "show cursor: {s:?}");
    }

    #[test]
    fn terminal_draws_board_over_remote_backend_and_autoresizes() {
        use crate::app::App;
        use crate::data::Data;
        use ratatui::Terminal;

        let buf = SharedBuf::default();
        let (backend, size) = RemoteBackend::new(buf.clone(), 100, 28).unwrap();
        let mut terminal = Terminal::new(backend).unwrap();
        let data = Data::open_in_memory_for_test();
        data.seed_project_issue("CLI", "First");
        let mut app = App::new();
        crate::runtime::reload(&data, &mut app).unwrap();
        terminal.draw(|f| crate::ui::render(f, &app)).unwrap();
        let s = String::from_utf8_lossy(&buf.contents()).into_owned();
        assert!(s.contains("BACKLOG"), "board frame over the channel: {s:?}");

        // Resize via the shared cell; the next draw picks it up (autoresize).
        *size.lock().unwrap() = (60, 12);
        terminal.draw(|f| crate::ui::render(f, &app)).unwrap();
        assert_eq!(terminal.size().unwrap().width, 60);
    }
}
