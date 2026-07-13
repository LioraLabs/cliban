//! `cliban label` subcommands. Output is byte-for-byte parity with the Go
//! oracle (`internal/cli/label.go`).

use cliban_core::contexts::{labels, projects};

use crate::errors::CliResult;
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
        #[arg(long)]
        project: String,
    },
    /// List labels for a project
    Ls {
        #[arg(long)]
        project: String,
        #[arg(long)]
        json: bool,
    },
    /// Delete a label (detaches it from all issues)
    Rm {
        name: String,
        #[arg(long)]
        project: String,
    },
}

pub async fn run(db: &Option<String>, args: LabelArgs) -> CliResult<()> {
    match args.cmd {
        LabelCmd::Add { name, project } => add(db, name, project).await,
        LabelCmd::Ls { project, json } => ls(db, project, json).await,
        LabelCmd::Rm { name, project } => rm(db, name, project).await,
    }
}

async fn add(db: &Option<String>, name: String, project: String) -> CliResult<()> {
    let project = project.to_uppercase();
    let store = store_open::open(db).await?;
    store
        .call(move |conn| labels::create(conn, &project, &name))
        .await?;
    Ok(())
}

async fn ls(db: &Option<String>, project: String, json: bool) -> CliResult<()> {
    let project = project.to_uppercase();
    let store = store_open::open(db).await?;
    let labels = store.call(move |conn| labels::list(conn, &project)).await?;
    for l in &labels {
        if json {
            println!("{}", serde_json::json!({ "name": l.name }));
        } else {
            println!("{}", l.name);
        }
    }
    Ok(())
}

async fn rm(db: &Option<String>, name: String, project: String) -> CliResult<()> {
    let project = project.to_uppercase();
    let store = store_open::open(db).await?;
    // `issues_labels.label_id` has `ON DELETE CASCADE` (see migrations.rs) and
    // the connection runs with `PRAGMA foreign_keys = ON`, so deleting the
    // label row also detaches it from every issue.
    store
        .call(move |conn| {
            if let Some(p) = projects::get_by_key(conn, &project)? {
                conn.execute(
                    "DELETE FROM labels WHERE project_id = ?1 AND name = ?2",
                    (p.id, &name),
                )?;
            }
            Ok(())
        })
        .await?;
    Ok(())
}
