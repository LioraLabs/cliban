//! russh server plumbing: pubkey auth against the tenancy registry +
//! per-session handlers. Exec routes to the control-command router; shell
//! serves the board TUI to enrolled keys over the channel (picker first
//! when a key belongs to several tenants).

use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::Arc;

use cliban_tenancy::{Caps, Tenant, TenantManager, User};
use cliban_tui::remote::RemoteInput;
use russh::keys::ssh_key::HashAlg;
use russh::keys::PrivateKey;
use russh::server::{Auth, ChannelOpenHandle, Handler, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet, Pty};

use crate::config::{ServerConfig, SignupPolicy};
use crate::ServerError;
use crate::{commands, shell};

/// Build the russh server config around the persisted host key.
pub fn russh_config(key: PrivateKey) -> Arc<russh::server::Config> {
    Arc::new(russh::server::Config {
        keys: vec![key],
        inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
        auth_rejection_time: std::time::Duration::from_secs(3),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        ..Default::default()
    })
}

/// Shared server state: tenancy routing + signup policy.
pub struct AppState {
    pub manager: TenantManager,
    pub signup_policy: SignupPolicy,
    pub signup_token: Option<String>,
}

impl AppState {
    /// Open the tenancy layer under the configured data dir. Config caps use
    /// 0 = unlimited; the tenancy layer wants actual bounds.
    pub fn from_config(cfg: &ServerConfig) -> Result<AppState, ServerError> {
        fn cap(v: u32) -> i64 {
            if v == 0 {
                i64::MAX
            } else {
                i64::from(v)
            }
        }
        let manager = TenantManager::open(
            &cfg.data_dir,
            Caps {
                max_tenants_per_user: cap(cfg.max_tenants_per_key),
                max_tenants_global: cap(cfg.max_tenants),
            },
        )?;
        Ok(AppState {
            manager,
            signup_policy: cfg.signup_policy,
            signup_token: cfg.signup_token.clone(),
        })
    }
}

/// The authenticated key for one connection, resolved at auth time.
pub struct KeyInfo {
    /// "SHA256:..." fingerprint — the registry lookup handle.
    pub fingerprint: String,
    /// Full OpenSSH-encoded public key, stored on enrollment.
    pub openssh: String,
    /// SSH username offered at auth; becomes the display name on enrollment.
    pub username: String,
    /// Registry user, `Some` iff the key is enrolled. Signup/accept set it.
    pub user: Option<User>,
}

/// Connection factory handed to `russh::server::Server::run_on_socket`.
pub struct ClibandServer {
    pub state: Arc<AppState>,
}

impl russh::server::Server for ClibandServer {
    type Handler = SessionHandler;

    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> SessionHandler {
        SessionHandler {
            state: self.state.clone(),
            key: None,
            ptys: HashMap::new(),
            boards: HashMap::new(),
        }
    }
}

/// Per-connection handler.
pub struct SessionHandler {
    state: Arc<AppState>,
    /// Set by `auth_publickey`; `None` until auth completes.
    key: Option<KeyInfo>,
    /// pty size per channel (pty-req arrives before the shell request).
    ptys: HashMap<ChannelId, (u16, u16)>,
    /// Live board sessions: input senders keyed by channel. Dropping a
    /// sender is the teardown signal for that channel's board task.
    boards: HashMap<ChannelId, mpsc::Sender<RemoteInput>>,
}

/// SSH sends u32 dimensions; terminals are u16 and zero is useless.
fn dim(v: u32) -> u16 {
    u16::try_from(v).unwrap_or(u16::MAX).max(1)
}

/// Reject, telling the client publickey is the only way in.
fn pubkey_only() -> Auth {
    let mut methods = MethodSet::empty();
    methods.push(MethodKind::PublicKey);
    Auth::Reject {
        proceed_with_methods: Some(methods),
        partial_success: false,
    }
}

impl Handler for SessionHandler {
    type Error = ServerError;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        Ok(pubkey_only())
    }

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(pubkey_only())
    }

    /// Accept every well-signed key: unknown keys must be able to connect to
    /// run `signup`/`accept`. Known keys get their registry user attached;
    /// command-level gating happens in the router.
    async fn auth_publickey(
        &mut self,
        user: &str,
        key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let openssh = key.to_openssh()?;
        let known = self
            .state
            .manager
            .registry()
            .user_for_pubkey(&fingerprint)?;
        self.key = Some(KeyInfo {
            fingerprint,
            openssh,
            username: user.to_string(),
            user: known,
        });
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.ptys.insert(channel, (dim(col_width), dim(row_height)));
        session.channel_success(channel)?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let size = (dim(col_width), dim(row_height));
        // Track the size even before a shell starts, so a resize in the
        // pty-req -> shell gap isn't lost; then forward to a live board.
        self.ptys.insert(channel, size);
        if let Some(tx) = self.boards.get(&channel) {
            let _ = tx.send(RemoteInput::Resize(size.0, size.1));
        }
        Ok(())
    }

    /// Enrolled keys with a pty get the board (picker first when they belong
    /// to several tenants); unknown keys get usage guidance and a closed
    /// channel; pty-less requests are told to reconnect with `ssh -t`.
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;

        // Re-checked per shell request: exec commands (signup/accept) can
        // enroll the key mid-connection.
        let Some(user) = self.key.as_ref().and_then(|k| k.user.clone()) else {
            // \r\n: the client usually has a pty in raw mode here.
            let text = commands::USAGE.replace('\n', "\r\n");
            session.data(channel, text.into_bytes())?;
            session.exit_status_request(channel, 1)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        };

        let Some(&size) = self.ptys.get(&channel) else {
            session.data(channel, shell::NO_TTY.as_bytes().to_vec())?;
            session.exit_status_request(channel, 1)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        };

        if self.boards.contains_key(&channel) {
            return Ok(()); // one shell per channel; ignore repeats
        }

        let tenants: Vec<Tenant> = match self.state.manager.registry().tenants_for_user(user.id) {
            Ok(ts) => ts.into_iter().map(|(t, _)| t).collect(),
            Err(e) => {
                // Same posture as commands::tenancy_fail: log details
                // server-side, answer generically.
                eprintln!("cliband: shell: {e}");
                session.data(channel, b"cliband: internal error\r\n".to_vec())?;
                session.exit_status_request(channel, 1)?;
                session.eof(channel)?;
                session.close(channel)?;
                return Ok(());
            }
        };
        if tenants.is_empty() {
            let text = format!(
                "no boards yet — create or join one first:\r\n\r\n{}",
                commands::USAGE.replace('\n', "\r\n")
            );
            session.data(channel, text.into_bytes())?;
            session.exit_status_request(channel, 1)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }

        let (tx, rx) = mpsc::channel();
        self.boards.insert(channel, tx);
        let task = shell::BoardTask {
            rt: tokio::runtime::Handle::current(),
            state: self.state.clone(),
            handle: session.handle(),
            channel,
            size,
            tenants,
            input: rx,
        };
        tokio::task::spawn_blocking(move || shell::run_board(task));
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        let line = String::from_utf8_lossy(data);
        let out = match self.key.as_mut() {
            Some(key) => commands::run(&self.state, key, &line),
            // Unreachable in practice: exec only arrives post-auth.
            None => commands::Output {
                text: commands::USAGE.to_string(),
                exit: 1,
            },
        };
        session.data(channel, out.text.into_bytes())?;
        session.exit_status_request(channel, out.exit)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }

    /// Channel bytes are the board's input stream.
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(tx) = self.boards.get(&channel) {
            let _ = tx.send(RemoteInput::Bytes(data.to_vec()));
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.boards.remove(&channel);
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.boards.remove(&channel);
        self.ptys.remove(&channel);
        Ok(())
    }
}
