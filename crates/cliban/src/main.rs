use clap::Parser;

mod cmd;
mod descmd;

mod audit;
mod errors;
mod lint;
mod output;
mod search;
mod since;
mod stdin_input;
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

    // ------------------------------------------------------------------
    // Unix spares: the issue is cliban's default noun, so the bare verbs
    // work — `cliban ls|mv|rm|show|log|tick|cat` forward to `issue <verb>`
    // at the clap level (same Args structs, same handlers), byte-identical
    // in args, flags, output, and exit codes.
    //
    // Admission test for any spare or reflex alias (here and on the issue
    // namespace): it must be a COMMON GUESS an agent or human would actually
    // type, AND have exactly one UNAMBIGUOUS SAFE INTERPRETATION — doing the
    // closest safe thing beats losing a turn to a usage error, but only when
    // there is nothing to mis-guess. Explicitly rejected under that test:
    //   * `ln`    — blocks vs related is a real ambiguity; a guessed link
    //               could silently create the wrong relation.
    //   * `touch` — issue keys are generated, so there is nothing safe for
    //               a bare `touch KEY` to create.
    // All spares are hidden from --help: the canonical surface is what the
    // docs and the skill teach; these only catch the reflex.
    // ------------------------------------------------------------------
    /// (hidden) `cliban ls` = `cliban issue ls`
    #[command(hide = true)]
    Ls(cmd::issue::LsArgs),
    /// (hidden) `cliban mv` = `cliban issue mv`
    #[command(hide = true)]
    Mv(cmd::issue::MvArgs),
    /// (hidden) `cliban rm` = `cliban issue rm` (archives; never deletes)
    #[command(hide = true)]
    Rm(cmd::issue::RmArgs),
    /// (hidden) `cliban show` = `cliban issue show`
    #[command(hide = true)]
    Show(cmd::issue::ShowArgs),
    /// (hidden) `cliban log` = `cliban issue log`
    #[command(hide = true)]
    Log(cmd::issue::LogArgs),
    /// (hidden) `cliban tick` = `cliban issue tick`
    #[command(hide = true)]
    Tick(cmd::issue::TickArgs),
    /// (hidden) `cliban cat` = `cliban issue cat`
    #[command(hide = true)]
    Cat(cmd::issue::CatArgs),
}

/// Rewrap a spare top-level verb as its canonical `issue <verb>` — the whole
/// forwarding layer is this one constructor call; the handlers never know.
fn issue_default(cmd: cmd::issue::IssueCmd) -> cmd::issue::IssueArgs {
    cmd::issue::IssueArgs { cmd }
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
        Some(Command::Ls(a)) => {
            cmd::issue::run(&cli.db, issue_default(cmd::issue::IssueCmd::Ls(a))).await
        }
        Some(Command::Mv(a)) => {
            cmd::issue::run(&cli.db, issue_default(cmd::issue::IssueCmd::Mv(a))).await
        }
        Some(Command::Rm(a)) => {
            cmd::issue::run(&cli.db, issue_default(cmd::issue::IssueCmd::Rm(a))).await
        }
        Some(Command::Show(a)) => {
            cmd::issue::run(&cli.db, issue_default(cmd::issue::IssueCmd::Show(a))).await
        }
        Some(Command::Log(a)) => {
            cmd::issue::run(&cli.db, issue_default(cmd::issue::IssueCmd::Log(a))).await
        }
        Some(Command::Tick(a)) => {
            cmd::issue::run(&cli.db, issue_default(cmd::issue::IssueCmd::Tick(a))).await
        }
        Some(Command::Cat(a)) => {
            cmd::issue::run(&cli.db, issue_default(cmd::issue::IssueCmd::Cat(a))).await
        }
    }
}
