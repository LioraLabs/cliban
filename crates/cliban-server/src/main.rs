use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use russh::server::Server as _;
use tokio::signal::unix::{signal, SignalKind};

use cliban_server::config::{ServerConfig, SignupPolicy};
use cliban_server::hostkey;
use cliban_server::server::{russh_config, AppState, ClibandServer};

#[derive(Parser)]
#[command(
    name = "cliband",
    version,
    about = "cliban SSH daemon — hosted shared boards"
)]
struct Cli {
    /// path to TOML config file (built-in defaults when omitted)
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let cfg = match &cli.config {
        Some(path) => ServerConfig::load(path)?,
        None => ServerConfig::default(),
    };

    let state = Arc::new(AppState::from_config(&cfg)?);
    let key = hostkey::load_or_generate(&cfg.data_dir)?;
    let listener = tokio::net::TcpListener::bind(&cfg.listen_addr).await?;
    // One fact per line on stderr; journald stamps, tags, and indexes these
    // by unit, so no logging framework is needed.
    eprintln!(
        "cliband {} listening on {}",
        env!("CARGO_PKG_VERSION"),
        listener.local_addr()?
    );
    eprintln!("cliband: data dir {}", cfg.data_dir.display());
    eprintln!("cliband: signup policy {:?}", cfg.signup_policy);
    if cfg.signup_policy == SignupPolicy::Token && cfg.signup_token.is_none() {
        eprintln!("cliband: signup_token is unset — signup is denied until one is configured");
    }

    let mut server = ClibandServer { state };
    let running = server.run_on_socket(russh_config(key), &listener);
    let handle = running.handle();

    tokio::spawn(async move {
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = sigterm.recv() => handle.shutdown("SIGTERM".into()),
            _ = tokio::signal::ctrl_c() => handle.shutdown("SIGINT".into()),
        }
    });

    running.await?;
    eprintln!("cliband: shut down");
    Ok(())
}
