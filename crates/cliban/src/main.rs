use clap::Parser;

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
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        None | Some(Command::Tui) => {
            // TODO(CLI-8 integration): cliban board / no-args launches cliban-tui.
            println!("TUI not yet wired");
        }
    }
}
