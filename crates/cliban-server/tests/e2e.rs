use std::sync::Arc;
use std::time::Duration;

use cliban_core::contexts::{issues, projects};
use cliban_server::commands::USAGE;
use cliban_server::config::ServerConfig;
use cliban_server::server::{russh_config, AppState, ClibandServer};
use russh::keys::{Algorithm, PrivateKey, PrivateKeyWithHashAlg};
use russh::server::Server as _;
use russh::ChannelMsg;
use tokio::sync::broadcast;

struct TrustingClient;

impl russh::client::Handler for TrustingClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

fn temp_data_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cliband-e2e-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn start_server() -> (
    std::net::SocketAddr,
    russh::server::RunningServerHandle,
    tokio::task::JoinHandle<std::io::Result<()>>,
) {
    let (addr, handle, join, _state) = start_server_with_state().await;
    (addr, handle, join)
}

/// Like [`start_server`], but also hands back the server's `AppState` so a
/// test can reach the tenant manager directly (seeding data, inspecting a
/// tenant's change feed).
async fn start_server_with_state() -> (
    std::net::SocketAddr,
    russh::server::RunningServerHandle,
    tokio::task::JoinHandle<std::io::Result<()>>,
    Arc<AppState>,
) {
    let cfg = ServerConfig {
        data_dir: temp_data_dir(),
        signup_token: Some("sesame".into()),
        ..ServerConfig::default()
    };
    let state = Arc::new(AppState::from_config(&cfg).unwrap());
    let server_state = state.clone();
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let join = tokio::spawn(async move {
        let mut server = ClibandServer {
            state: server_state,
        };
        let running = server.run_on_socket(russh_config(key), &listener);
        let _ = tx.send(running.handle());
        running.await
    });
    (addr, rx.await.unwrap(), join, state)
}

/// Seed one project holding one backlog issue into a tenant's DB through the
/// server's own tenant manager — the same cached store the board sessions
/// use, so no publish is needed (sessions opened afterwards read it fresh).
async fn seed_issue(state: &AppState, slug: &str, title: &str) {
    let tenant = state
        .manager
        .registry()
        .tenant_by_slug(slug)
        .unwrap()
        .expect("tenant exists");
    let handle = state.manager.handle(&tenant.id).unwrap();
    let title = title.to_string();
    handle
        .store
        .call(move |conn| {
            projects::create(
                conn,
                projects::CreateProject {
                    key: "LIVE".into(),
                    name: "LIVE".into(),
                    ..Default::default()
                },
            )?;
            issues::create(
                conn,
                "LIVE",
                issues::CreateIssue {
                    title,
                    ..Default::default()
                },
            )?;
            Ok(())
        })
        .await
        .unwrap();
}

fn client_key() -> PrivateKey {
    PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap()
}

async fn connect(
    addr: std::net::SocketAddr,
    user: &str,
    key: &PrivateKey,
) -> russh::client::Handle<TrustingClient> {
    let cfg = Arc::new(russh::client::Config::default());
    let mut session = russh::client::connect(cfg, addr, TrustingClient)
        .await
        .unwrap();
    let auth = session
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(Arc::new(key.clone()), None),
        )
        .await
        .unwrap();
    assert!(auth.success());
    session
}

/// Skip Success/WindowAdjusted/etc.; return the next Data payload.
async fn next_data(channel: &mut russh::Channel<russh::client::Msg>) -> Vec<u8> {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), channel.wait())
            .await
            .expect("timed out waiting for channel message")
            .expect("channel closed before data arrived");
        if let ChannelMsg::Data { data } = msg {
            return data.to_vec();
        }
    }
}

/// Drop ANSI escape sequences (CSI and two-byte ESC sequences) so tests can
/// assert on rendered text.
///
/// Ratatui only writes cells that changed from the previous frame; on a
/// fresh alternate screen the "previous" frame is blank, so a run of blank
/// cells (e.g. the spaces inside a title like `" pick a board "`) is never
/// written at all — the renderer just cursor-jumps past it. Left alone,
/// that glues adjacent words together ("pickaboard"). To keep needles like
/// `"pick a board"` meaningful, an absolute cursor-position escape
/// (`CSI row;col H`/`f`) that jumps forward within the same row is turned
/// back into literal spaces.
fn strip_ansi(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    let mut cursor: Option<(u32, u32)> = None;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if bytes.get(i) == Some(&b'[') {
                i += 1;
                let start = i;
                while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'H') || bytes.get(i) == Some(&b'f') {
                    let params = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                    let mut parts = params.splitn(2, ';');
                    let row: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                    let col: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                    if let Some((last_row, last_col)) = cursor {
                        if row == last_row && col > last_col {
                            for _ in 0..(col - last_col) {
                                out.push(' ');
                            }
                        }
                    }
                    cursor = Some((row, col));
                }
            }
            i += 1; // final byte (or the single char after ESC)
        } else {
            out.push(bytes[i] as char);
            if let Some((_, col)) = cursor.as_mut() {
                *col += 1;
            }
            i += 1;
        }
    }
    out
}

/// Accumulate channel data until the stripped text contains `needle`.
async fn read_until(channel: &mut russh::Channel<russh::client::Msg>, needle: &str) -> String {
    let mut text = String::new();
    for _ in 0..200 {
        text.push_str(&strip_ansi(&next_data(channel).await));
        if text.contains(needle) {
            return text;
        }
    }
    panic!("never saw {needle:?} on the channel; got:\n{text}");
}

/// Open a session channel with a pty and a shell, as `ssh host` would.
async fn open_shell(
    session: &russh::client::Handle<TrustingClient>,
) -> russh::Channel<russh::client::Msg> {
    let channel = session.channel_open_session().await.unwrap();
    channel
        .request_pty(true, "xterm-256color", 100, 30, 0, 0, &[])
        .await
        .unwrap();
    channel.request_shell(true).await.unwrap();
    channel
}

/// Drain to Close; returns (raw bytes seen, exit status if any). Bounded by
/// a total deadline, not per-message: a still-rendering board produces data
/// every tick, so a per-message timeout alone can never fire.
async fn drain_to_close(
    channel: &mut russh::Channel<russh::client::Msg>,
) -> (Vec<u8>, Option<u32>) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut raw = Vec::new();
    let mut status = None;
    loop {
        let msg = tokio::time::timeout_at(deadline, channel.wait())
            .await
            .expect("timed out draining channel (no Close before deadline)");
        match msg {
            Some(ChannelMsg::Data { data }) => raw.extend_from_slice(&data),
            Some(ChannelMsg::ExitStatus { exit_status }) => status = Some(exit_status),
            Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    (raw, status)
}

async fn exec(session: &russh::client::Handle<TrustingClient>, cmd: &str) -> (String, u32) {
    let mut channel = session.channel_open_session().await.unwrap();
    channel.exec(true, cmd).await.unwrap();
    let mut out = Vec::new();
    let mut status = None;
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), channel.wait())
            .await
            .expect("timed out draining exec channel");
        match msg {
            Some(ChannelMsg::Data { data }) => out.extend_from_slice(&data),
            Some(ChannelMsg::ExitStatus { exit_status }) => status = Some(exit_status),
            Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    (
        String::from_utf8(out).unwrap(),
        status.expect("no exit status"),
    )
}

#[tokio::test]
async fn unrouted_exec_prints_usage_with_exit_1() {
    let (addr, handle, join) = start_server().await;
    let session = connect(addr, "tester", &client_key()).await;
    let (out, status) = exec(&session, "cliban issue list").await;
    assert_eq!(out, USAGE);
    assert_eq!(status, 1);
    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn pty_shell_serves_board_resizes_and_quits_cleanly() {
    let (addr, handle, join) = start_server().await;
    let session = connect(addr, "alice", &client_key()).await;
    let (out, status) = exec(&session, "signup team-shell sesame").await;
    assert_eq!(status, 0, "{out}");

    let mut channel = open_shell(&session).await;
    // Single tenant: straight to the board.
    read_until(&mut channel, "BACKLOG").await;

    // window-change must be accepted and the session must stay live:
    // the help overlay still opens afterwards.
    channel.window_change(140, 45, 0, 0).await.unwrap();
    channel.data(&b"?"[..]).await.unwrap();
    read_until(&mut channel, "Help").await;

    // In the help overlay any key closes it, so plain "qqy" is deterministic
    // regardless of how the transport chunks bytes: q closes help, q opens
    // confirm-quit, y confirms. (Esc is deliberately avoided here: an ESC
    // byte followed closely by more bytes parses as Alt+<char>, exactly like
    // a local crossterm terminal.)
    channel.data(&b"qqy"[..]).await.unwrap();
    let (raw, status) = drain_to_close(&mut channel).await;
    assert_eq!(status, Some(0));
    // Teardown restored the client's screen.
    let tail = String::from_utf8_lossy(&raw);
    assert!(
        tail.contains("\x1b[?1049l"),
        "leave-alt-screen missing: {tail:?}"
    );

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn shell_from_unknown_key_prints_usage_guidance() {
    let (addr, handle, join) = start_server().await;
    let session = connect(addr, "drifter", &client_key()).await;
    let mut channel = session.channel_open_session().await.unwrap();
    channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .unwrap();
    channel.request_shell(true).await.unwrap();
    let text = String::from_utf8(next_data(&mut channel).await).unwrap();
    assert_eq!(text, USAGE.replace('\n', "\r\n"));

    // The server also reports failure and closes the channel — no shell.
    let mut status = None;
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), channel.wait())
            .await
            .expect("timed out draining shell channel");
        match msg {
            Some(ChannelMsg::ExitStatus { exit_status }) => status = Some(exit_status),
            Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    assert_eq!(status, Some(1));

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn full_signup_invite_accept_flow_over_ssh() {
    let (addr, handle, join) = start_server().await;

    // Alice signs up with the server's token.
    let alice = connect(addr, "alice", &client_key()).await;
    let (out, status) = exec(&alice, "signup team-a sesame").await;
    assert_eq!(status, 0, "{out}");

    // Wrong token is refused.
    let mallory = connect(addr, "mallory", &client_key()).await;
    let (out, status) = exec(&mallory, "signup team-m wrong").await;
    assert_eq!(status, 1, "{out}");

    // whoami reflects ownership.
    let (out, status) = exec(&alice, "whoami").await;
    assert_eq!(status, 0, "{out}");
    assert!(out.contains("team-a (owner)"), "{out}");

    // Alice mints an invite; the code is the last word of line 1.
    let (out, status) = exec(&alice, "invite").await;
    assert_eq!(status, 0, "{out}");
    let code = out.lines().next().unwrap().rsplit(' ').next().unwrap();
    assert_eq!(code.len(), 32, "{out}");

    // Bob (brand-new key) accepts and lands as member.
    let bob = connect(addr, "bob", &client_key()).await;
    let (out, status) = exec(&bob, &format!("accept {code}")).await;
    assert_eq!(status, 0, "{out}");
    let (out, status) = exec(&bob, "members").await;
    assert_eq!(status, 0, "{out}");
    assert!(out.contains("alice (owner)"), "{out}");
    assert!(out.contains("bob (member)"), "{out}");

    // The invite was one-time.
    let carol = connect(addr, "carol", &client_key()).await;
    let (_, status) = exec(&carol, &format!("accept {code}")).await;
    assert_eq!(status, 1);

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn none_and_password_auth_are_rejected_pubkey_accepted() {
    let (addr, handle, join) = start_server().await;
    let cfg = Arc::new(russh::client::Config::default());
    let mut session = russh::client::connect(cfg, addr, TrustingClient)
        .await
        .unwrap();

    let none = session.authenticate_none("tester").await.unwrap();
    assert!(!none.success());
    let pw = session
        .authenticate_password("tester", "hunter2")
        .await
        .unwrap();
    assert!(!pw.success());

    let key = client_key();
    let auth = session
        .authenticate_publickey("tester", PrivateKeyWithHashAlg::new(Arc::new(key), None))
        .await
        .unwrap();
    assert!(auth.success());

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn shell_without_pty_asks_for_a_tty_and_closes() {
    let (addr, handle, join) = start_server().await;
    let session = connect(addr, "alice", &client_key()).await;
    let (out, status) = exec(&session, "signup team-nopty sesame").await;
    assert_eq!(status, 0, "{out}");

    let mut channel = session.channel_open_session().await.unwrap();
    channel.request_shell(true).await.unwrap(); // no pty-req on purpose
    let text = read_until(&mut channel, "requires a TTY").await;
    assert!(text.contains("ssh -t"), "{text}");
    let (_, status) = drain_to_close(&mut channel).await;
    assert_eq!(status, Some(1));

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn multi_tenant_shell_shows_picker_then_picked_board() {
    let (addr, handle, join) = start_server().await;
    let session = connect(addr, "alice", &client_key()).await;
    let (out, status) = exec(&session, "signup team-a sesame").await;
    assert_eq!(status, 0, "{out}");
    let (out, status) = exec(&session, "signup team-b sesame").await;
    assert_eq!(status, 0, "{out}");

    let mut channel = open_shell(&session).await;
    let text = read_until(&mut channel, "team-b").await;
    assert!(text.contains("team-a"), "picker lists both tenants: {text}");
    assert!(text.contains("pick a board"), "{text}");

    // Down to team-b, Enter: the board replaces the picker.
    channel.data(&b"j\r"[..]).await.unwrap();
    read_until(&mut channel, "BACKLOG").await;

    channel.data(&b"qy"[..]).await.unwrap();
    let (_, status) = drain_to_close(&mut channel).await;
    assert_eq!(status, Some(0));

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn cancelling_the_picker_hangs_up_cleanly() {
    let (addr, handle, join) = start_server().await;
    let session = connect(addr, "alice", &client_key()).await;
    for slug in ["team-x", "team-y"] {
        let (out, status) = exec(&session, &format!("signup {slug} sesame")).await;
        assert_eq!(status, 0, "{out}");
    }

    let mut channel = open_shell(&session).await;
    read_until(&mut channel, "pick a board").await;
    channel.data(&b"q"[..]).await.unwrap();
    let (raw, status) = drain_to_close(&mut channel).await;
    assert_eq!(status, Some(0));
    let tail = String::from_utf8_lossy(&raw);
    assert!(tail.contains("\x1b[?1049l"), "screen restored: {tail:?}");

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn enrolled_key_with_no_memberships_gets_guidance() {
    let (addr, handle, join) = start_server().await;
    let session = connect(addr, "loner", &client_key()).await;
    // A failed accept still enrolls the key (documented behavior), leaving
    // it with zero memberships.
    let (_, status) = exec(&session, "accept bogus-code").await;
    assert_eq!(status, 1);

    let mut channel = open_shell(&session).await;
    let text = read_until(&mut channel, "no boards yet").await;
    assert!(text.contains("signup"), "{text}");
    let (_, status) = drain_to_close(&mut channel).await;
    assert_eq!(status, Some(1));

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_move_in_one_session_appears_in_the_other_without_input() {
    let (addr, handle, join, state) = start_server_with_state().await;

    let key = client_key();
    let alice = connect(addr, "alice", &key).await;
    let (out, status) = exec(&alice, "signup team-live sesame").await;
    assert_eq!(status, 0, "{out}");
    // A bystander tenant whose change feed must stay silent throughout.
    let bob = connect(addr, "bob", &client_key()).await;
    let (out, status) = exec(&bob, "signup team-other sesame").await;
    assert_eq!(status, 0, "{out}");

    seed_issue(&state, "team-live", "live-wire").await;
    let other = state
        .manager
        .registry()
        .tenant_by_slug("team-other")
        .unwrap()
        .unwrap();
    let mut other_feed = state.manager.handle(&other.id).unwrap().changes.subscribe();

    // Two independent connections, two boards on the same tenant. Each
    // read_until consumes that session's initial render, so anything read
    // afterwards can only come from a live refresh.
    let alice_again = connect(addr, "alice", &key).await;
    let mut a = open_shell(&alice).await;
    read_until(&mut a, "live-wire").await;
    let mut b = open_shell(&alice_again).await;
    read_until(&mut b, "live-wire").await;

    // A moves the focused card right (backlog -> in-progress). B's board
    // polls its change feed at the 100ms tick and re-renders the card in
    // its new column — with no input ever sent on B's channel. The bound
    // is generous for CI; the mechanism answers in ~a tick.
    a.data(&b"L"[..]).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), read_until(&mut b, "live-wire"))
        .await
        .expect("session B never refreshed after A's move");

    // Tenant isolation: team-other's feed saw nothing.
    match other_feed.try_recv() {
        Err(broadcast::error::TryRecvError::Empty) => {}
        got => panic!("cross-tenant leak: team-other's feed got {got:?}"),
    }

    // Both sessions still quit cleanly.
    a.data(&b"qy"[..]).await.unwrap();
    b.data(&b"qy"[..]).await.unwrap();
    let (_, sa) = drain_to_close(&mut a).await;
    let (_, sb) = drain_to_close(&mut b).await;
    assert_eq!(sa, Some(0));
    assert_eq!(sb, Some(0));

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}

#[tokio::test]
async fn client_disconnect_mid_board_leaves_server_healthy() {
    let (addr, handle, join) = start_server().await;
    let key = client_key();
    let session = connect(addr, "alice", &key).await;
    let (out, status) = exec(&session, "signup team-drop sesame").await;
    assert_eq!(status, 0, "{out}");

    let mut channel = open_shell(&session).await;
    read_until(&mut channel, "BACKLOG").await;
    // Vanish without closing the channel.
    session
        .disconnect(russh::Disconnect::ByApplication, "gone", "")
        .await
        .unwrap();

    // The server keeps serving: the same key reconnects onto the board.
    let session = connect(addr, "alice", &key).await;
    let mut channel = open_shell(&session).await;
    read_until(&mut channel, "BACKLOG").await;
    channel.data(&b"qy"[..]).await.unwrap();
    let (_, status) = drain_to_close(&mut channel).await;
    assert_eq!(status, Some(0));

    handle.shutdown("test done".into());
    join.await.unwrap().unwrap();
}
