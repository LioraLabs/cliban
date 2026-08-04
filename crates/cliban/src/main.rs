use clap::Parser;

mod cmd;
mod descmd;

mod audit;
mod errors;
mod lint;
mod output;
mod search;
mod since;
mod store_open;

#[derive(Parser)]
#[command(
    name = "cliban",
    version,
    about = "AI-agent-first kanban board for the terminal"
)]
struct Cli {
    /// path to SQLite DB (default: $CLIBAN_DB or $XDG_DATA_HOME/cliban/cliban.db)
    #[arg(long, global = true)]
    db: Option<String>,
    #[command(subcommand)]
    cmd: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Open the kanban TUI
    Tui,
    /// Manage projects
    Project(cmd::project::ProjectArgs),
    /// Manage labels
    Label(cmd::label::LabelArgs),
    /// Manage issues
    Issue(Box<cmd::issue::IssueArgs>),
    /// What changed on the board recently (newest first)
    Activity(cmd::activity::ActivityArgs),
    /// Manage milestones
    Milestone(cmd::milestone::MilestoneArgs),
    /// Import an issue from an external tracker
    #[cfg(feature = "linear")]
    Import(cmd::sync::ImportArgs),
    /// Push a cliban issue back to an external tracker
    #[cfg(feature = "linear")]
    Push(cmd::sync::PushArgs),
    /// Refresh every linked issue from an external tracker in one call
    #[cfg(feature = "linear")]
    Sync(cmd::sync::SyncArgs),
}

/// Restore SIGPIPE's default disposition, which Rust's runtime sets to
/// ignored before `main`.
///
/// With SIGPIPE ignored, a write past a closed pipe (`cliban activity --json |
/// head -3` on a busy board) returns EPIPE and `println!` panics with "failed
/// printing to stdout: Broken pipe". With the default disposition the process
/// dies quietly on that write, exactly like cat/grep/git — and the shell
/// reports the last pipeline stage's status, so the pipeline still succeeds.
///
/// Done here, once, rather than mapping `BrokenPipe` errors to exit 0 at
/// every stdout write site: one line covers every command and every current
/// and future print path.
#[cfg(unix)]
fn reset_sigpipe() {
    // SAFETY: signal(2) with SIG_DFL only resets a signal disposition; it is
    // called before any threads are spawned and cannot violate memory safety.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

fn main() {
    reset_sigpipe();

    let cli = Cli::parse();

    // The TUI is synchronous and owns its own runtime (see cliban-tui::data),
    // so it must run OUTSIDE a tokio runtime — launch it before we build one.
    if matches!(cli.cmd, None | Some(Command::Tui)) {
        let path = store_open::db_path(&cli.db);
        if let Err(e) = cliban_tui::run(path) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");
    if let Err(e) = rt.block_on(run(cli)) {
        eprintln!("error: {}", e.message());
        std::process::exit(e.code());
    }
}

async fn run(cli: Cli) -> errors::CliResult<()> {
    match cli.cmd {
        None | Some(Command::Tui) => unreachable!("TUI handled in main before runtime"),
        Some(Command::Project(args)) => cmd::project::run(&cli.db, args).await,
        Some(Command::Label(args)) => cmd::label::run(&cli.db, args).await,
        Some(Command::Issue(args)) => cmd::issue::run(&cli.db, *args).await,
        Some(Command::Activity(args)) => cmd::activity::run(&cli.db, args).await,
        Some(Command::Milestone(args)) => cmd::milestone::run(&cli.db, args).await,
        #[cfg(feature = "linear")]
        Some(Command::Import(args)) => cmd::sync::run_import(&cli.db, args).await,
        #[cfg(feature = "linear")]
        Some(Command::Push(args)) => cmd::sync::run_push(&cli.db, args).await,
        #[cfg(feature = "linear")]
        Some(Command::Sync(args)) => cmd::sync::run_sync(&cli.db, args).await,
    }
}
