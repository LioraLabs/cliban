//! `cliban label` subcommands.

use cliban_core::contexts::{labels, projects};

use crate::errors::CliResult;
use crate::output::Mode;
use crate::store_open;

#[derive(clap::Args)]
pub struct LabelArgs {
    #[command(subcommand)]
    pub cmd: LabelCmd,
}

#[derive(clap::Subcommand)]
pub enum LabelCmd {
    /// Add a label to a project
    Add {
        name: String,
        /// project key (default: $CLIBAN_PROJECT)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// JSON output (echo the created label)
        #[arg(long)]
        json: bool,
        /// human output (one-line confirmation)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// List labels for a project
    Ls {
        /// project key (default: $CLIBAN_PROJECT)
        #[arg(long, short = 'p')]
        project: Option<String>,
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Delete a label (detaches it from all issues)
    Rm {
        name: String,
        /// project key (default: $CLIBAN_PROJECT)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// JSON output (echo the removal)
        #[arg(long)]
        json: bool,
        /// human output (one-line confirmation)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
}

pub async fn run(db: &Option<String>, args: LabelArgs) -> CliResult<()> {
    match args.cmd {
        LabelCmd::Add {
            name,
            project,
            json,
            table,
        } => add(db, name, project, crate::output::mode(json, table)).await,
        LabelCmd::Ls {
            project,
            json,
            table,
        } => ls(db, project, crate::output::mode(json, table)).await,
        LabelCmd::Rm {
            name,
            project,
            json,
            table,
        } => rm(db, name, project, crate::output::mode(json, table)).await,
    }
}

async fn add(
    db: &Option<String>,
    name: String,
    project: Option<String>,
    mode: Mode,
) -> CliResult<()> {
    let project = crate::scope::required_project(project)?;
    let store = store_open::open(db).await?;
    let create_project = project.clone();
    let create_name = name.clone();
    store
        .call(move |conn| labels::create(conn, &create_project, &create_name))
        .await?;
    if mode.is_json() {
        println!(
            "{}",
            serde_json::json!({ "name": name, "project": project })
        );
    } else {
        println!("added label {name} to {project}");
    }
    Ok(())
}

async fn ls(db: &Option<String>, project: Option<String>, mode: Mode) -> CliResult<()> {
    let project = crate::scope::required_project(project)?;
    let store = store_open::open(db).await?;
    let labels = store.call(move |conn| labels::list(conn, &project)).await?;
    for l in &labels {
        if mode.is_json() {
            println!("{}", serde_json::json!({ "name": l.name }));
        } else {
            println!("{}", l.name);
        }
    }
    Ok(())
}

async fn rm(
    db: &Option<String>,
    name: String,
    project: Option<String>,
    mode: Mode,
) -> CliResult<()> {
    let project = crate::scope::required_project(project)?;
    let store = store_open::open(db).await?;
    // `issues_labels.label_id` has `ON DELETE CASCADE` (see migrations.rs) and
    // the connection runs with `PRAGMA foreign_keys = ON`, so deleting the
    // label row also detaches it from every issue.
    let rm_project = project.clone();
    let rm_name = name.clone();
    store
        .call(move |conn| {
            if let Some(p) = projects::get_by_key(conn, &rm_project)? {
                conn.execute(
                    "DELETE FROM labels WHERE project_id = ?1 AND name = ?2",
                    (p.id, &rm_name),
                )?;
            }
            Ok(())
        })
        .await?;
    if mode.is_json() {
        println!(
            "{}",
            serde_json::json!({ "name": name, "project": project, "removed": true })
        );
    } else {
        println!("removed label {name} from {project}");
    }
    Ok(())
}
