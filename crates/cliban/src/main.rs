use clap::Parser;

mod cmd;
mod descmd;
use cliban::migrate;

mod errors;
mod output;
mod search;
mod store_open;

#[derive(clap::Args)]
struct MigrateLegacyArgs {
    /// path to the legacy Go SQLite db to read (read-only)
    #[arg(long)]
    from: String,
    /// path to the new cliban-core db to create (must not exist)
    #[arg(long)]
    to: String,
}

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
    /// Manage issues
    Issue(cmd::issue::IssueArgs),
    /// Manage milestones
    Milestone(cmd::milestone::MilestoneArgs),
    /// Fuzzy-find issues; print selected key to stdout
    Fff(cmd::fff::FffArgs),
    /// Migrate a legacy Go cliban db into a fresh cliban-core db
    MigrateLegacy(MigrateLegacyArgs),
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
        Some(Command::Issue(args)) => cmd::issue::run(&cli.db, args).await,
        Some(Command::Milestone(args)) => cmd::milestone::run(&cli.db, args).await,
        Some(Command::Fff(args)) => cmd::fff::run(&cli.db, args).await,
        Some(Command::MigrateLegacy(args)) => {
            let report = migrate::migrate(
                std::path::Path::new(&args.from),
                std::path::Path::new(&args.to),
            )
            .map_err(errors::CliError::other)?;
            println!(
                "migrated: {} projects, {} milestones, {} issues, {} labels, {} issue_labels, {} relations",
                report.projects, report.milestones, report.issues, report.labels,
                report.issues_labels, report.relations,
            );
            Ok(())
        }
    }
}
