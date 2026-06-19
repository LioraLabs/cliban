use clap::Parser;

mod cmd;
mod descmd;
mod errors;
mod output;
mod store_open;

#[derive(Parser)]
#[command(name = "cliban", about = "AI-agent-first kanban board for the terminal")]
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
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        eprintln!("error: {}", e.message());
        std::process::exit(e.code());
    }
}

async fn run(cli: Cli) -> errors::CliResult<()> {
    match cli.cmd {
        None | Some(Command::Tui) => {
            // TODO(CLI-8 integration): cliban board / no-args launches cliban-tui.
            println!("TUI not yet wired");
            Ok(())
        }
        Some(Command::Project(args)) => cmd::project::run(&cli.db, args).await,
        Some(Command::Label(args)) => cmd::label::run(&cli.db, args).await,
    }
}
