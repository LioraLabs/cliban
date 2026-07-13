//! Board-over-SSH: bridge one russh channel to the blocking cliban-tui
//! event loop on a dedicated blocking task.
//!
//! Wiring: the connection handler forwards channel data bytes and
//! window-change resizes into an mpsc ([`RemoteInput`]); the board task
//! renders through a [`RemoteBackend`] whose writer sends each flush to the
//! client via the session [`Handle`]. Teardown is symmetric: user quit ends
//! the loop (exit 0); client disconnect / channel close drops the sender,
//! the session errors out, and the task dies — no leaked tasks either way.

use std::io::{self, Write};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use cliban_tenancy::Tenant;
use cliban_tui::app::App;
use cliban_tui::data::Data;
use cliban_tui::remote::{ChannelSession, RemoteBackend, RemoteInput};
use cliban_tui::{picker, runtime};
use ratatui::Terminal;
use russh::server::Handle;
use russh::ChannelId;
use tokio::sync::broadcast;

use crate::server::AppState;

/// Sent when a shell is requested on a channel that never did pty-req.
pub const NO_TTY: &str = "cliband: the board requires a TTY; use ssh -t\r\n";

/// `io::Write` onto an SSH channel: buffer locally, send one channel-data
/// message per flush. `Handle::data` completes once the message enters the
/// session's mpsc (window-stalled data is buffered by russh server-side),
/// so the only backpressure is that 100-message queue. Known limitation: a
/// live client that stops reading TCP can park the session loop and, once
/// the queue fills, park this blocking thread until the TCP connection
/// itself dies — an accepted russh-level flow-control gap.
struct ChannelWriter {
    rt: tokio::runtime::Handle,
    handle: Handle,
    channel: ChannelId,
    buf: Vec<u8>,
}

impl Write for ChannelWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let data = std::mem::take(&mut self.buf);
        self.rt
            .block_on(self.handle.data(self.channel, data))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "ssh channel closed"))
    }
}

/// Everything the blocking board task needs, captured on the async side
/// (`Handle::current()` is only guaranteed there).
pub struct BoardTask {
    pub rt: tokio::runtime::Handle,
    pub state: Arc<AppState>,
    pub handle: Handle,
    pub channel: ChannelId,
    /// Initial pty size from pty-req.
    pub size: (u16, u16),
    /// The user's memberships; picker shown when more than one.
    pub tenants: Vec<Tenant>,
    pub input: Receiver<RemoteInput>,
}

/// Drive picker + board to completion, then hang up. Runs on a blocking
/// thread (`tokio::task::spawn_blocking`). All sends after the loop are
/// best-effort: the client may already be gone.
pub fn run_board(task: BoardTask) {
    let rt = task.rt.clone();
    let handle = task.handle.clone();
    let channel = task.channel;
    let exit = match board_session(task) {
        Ok(()) => 0,
        Err(e) => {
            // A vanished client surfaces as an io error on the input channel
            // or the writer — a normal ending, not worth a server log line.
            let disconnect = e.downcast_ref::<io::Error>().is_some_and(|io| {
                matches!(
                    io.kind(),
                    io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe
                )
            });
            if !disconnect {
                eprintln!("cliband: board session ended: {e}");
            }
            1
        }
    };
    rt.block_on(async {
        let _ = handle.exit_status_request(channel, exit).await;
        let _ = handle.eof(channel).await;
        let _ = handle.close(channel).await;
    });
}

fn board_session(task: BoardTask) -> Result<(), Box<dyn std::error::Error>> {
    let writer = ChannelWriter {
        rt: task.rt,
        handle: task.handle,
        channel: task.channel,
        buf: Vec::new(),
    };
    let (backend, size) = RemoteBackend::new(writer, task.size.0, task.size.1)?;
    let mut terminal = Terminal::new(backend)?;
    let mut session = ChannelSession::new(task.input, size);

    let res = (|| -> Result<(), Box<dyn std::error::Error>> {
        let picked = if task.tenants.len() == 1 {
            Some(0)
        } else {
            let slugs: Vec<String> = task.tenants.iter().map(|t| t.slug.clone()).collect();
            picker::pick(&mut terminal, &mut session, " pick a board ", &slugs)?
        };
        let Some(i) = picked else { return Ok(()) };
        // Shared cached Store: every session for this tenant funnels through
        // one writer thread, and each Data write is a single store.call —
        // no multi-call write sequence here, so the tenant write_lock is
        // not needed.
        let tenant_handle = task.state.manager.handle(&task.tenants[i].id)?;

        // Live updates. Subscribe to the tenant's change feed and poll it
        // from the session (100ms cadence via the event loop): draining
        // try_recv coalesces any burst of writes into one Refresh, and no
        // extra task means teardown semantics are untouched. Publish after
        // every local commit so sibling sessions refresh too.
        let mut feed = tenant_handle.changes.subscribe();
        session.set_dirty_check(move || {
            let mut dirty = false;
            loop {
                match feed.try_recv() {
                    Ok(()) => dirty = true,
                    // Lagged means "you missed some": still just a refresh.
                    Err(broadcast::error::TryRecvError::Lagged(_)) => dirty = true,
                    Err(_) => break, // Empty (or Closed): nothing pending
                }
            }
            dirty
        });
        let publish = tenant_handle.changes.clone();
        let mut data = Data::from_store(tenant_handle.store.clone())?;
        data.set_on_mutate(move || {
            // Err only means no subscriber is listening right now.
            let _ = publish.send(());
        });
        let mut app = App::new();
        runtime::reload(&data, &mut app)?;
        runtime::event_loop(&mut terminal, &mut session, &data, &mut app)
    })();

    // Restore the client's screen whatever happened; if the client is gone
    // this send just fails quietly.
    let _ = terminal.backend_mut().leave_screen();
    res
}
