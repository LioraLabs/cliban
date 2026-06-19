//! `cliban project` subcommands. Output is byte-for-byte parity with the Go
//! oracle (`internal/cli/project.go`).

use cliban_core::contexts::{issues, projects};
use cliban_core::contexts::issues::ListOpts;
use cliban_core::contexts::projects::{CreateProject, UpdateProject};
use cliban_core::time::format_usec;

use crate::errors::{CliError, CliResult};
use crate::output::build_project_json;
use crate::store_open;

#[derive(clap::Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub cmd: ProjectCmd,
}

#[derive(clap::Subcommand)]
pub enum ProjectCmd {
    /// Add a project (KEY must be uppercase letters/digits, 2-10 chars)
    Add {
        key: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List projects
    Ls {
        #[arg(long)]
        archived: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show a project
    Show {
        key: String,
        #[arg(long)]
        json: bool,
    },
    /// Edit a project
    Edit {
        key: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "auto-archive-done-after")]
        auto_archive_done_after: Option<String>,
    },
    /// Archive a project
    Archive { key: String },
    /// Unarchive a project
    Unarchive { key: String },
    /// Delete a project
    Rm {
        key: String,
        #[arg(long)]
        force: bool,
    },
}

pub async fn run(db: &Option<String>, args: ProjectArgs) -> CliResult<()> {
    match args.cmd {
        ProjectCmd::Add {
            key,
            name,
            description,
            json,
        } => add(db, key, name, description, json).await,
        ProjectCmd::Ls { archived, json } => ls(db, archived, json).await,
        ProjectCmd::Show { key, json } => show(db, key, json).await,
        ProjectCmd::Edit {
            key,
            name,
            description,
            auto_archive_done_after,
        } => edit(db, key, name, description, auto_archive_done_after).await,
        ProjectCmd::Archive { key } => set_archived(db, key, true).await,
        ProjectCmd::Unarchive { key } => set_archived(db, key, false).await,
        ProjectCmd::Rm { key, force } => rm(db, key, force).await,
    }
}

fn project_json(p: &cliban_core::schema::Project) -> serde_json::Value {
    build_project_json(
        &p.key,
        &p.name,
        &p.description,
        p.archived,
        p.auto_archive_done_after_days,
        p.issue_seq,
        &format_usec(p.inserted_at),
        &format_usec(p.updated_at),
    )
}

async fn add(
    db: &Option<String>,
    key: String,
    name: String,
    description: Option<String>,
    json: bool,
) -> CliResult<()> {
    let key = key.to_uppercase();
    let store = store_open::open(db).await?;
    let p = store
        .call(move |conn| {
            projects::create(
                conn,
                CreateProject {
                    key,
                    name,
                    description,
                    auto_archive_done_after_days: None,
                },
            )
        })
        .await?;
    if json {
        let v = project_json(&p);
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        println!("created project {} ({})", p.key, p.name);
    }
    Ok(())
}

async fn ls(db: &Option<String>, archived: bool, json: bool) -> CliResult<()> {
    let store = store_open::open(db).await?;
    let mut ps = store.call(projects::list).await?;
    if !archived {
        ps.retain(|p| !p.archived);
    }
    if json {
        for p in &ps {
            let v = project_json(p);
            println!("{}", serde_json::to_string(&v).unwrap());
        }
    } else {
        for p in &ps {
            let mark = if p.archived { " (archived)" } else { "" };
            println!("{:<10} {}{}", p.key, p.name, mark);
        }
    }
    Ok(())
}

async fn show(db: &Option<String>, key: String, json: bool) -> CliResult<()> {
    let key = key.to_uppercase();
    let store = store_open::open(db).await?;
    let p = store
        .call(move |conn| projects::fetch_by_key(conn, &key))
        .await?;
    if json {
        let v = project_json(&p);
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        println!("{} — {}\n{}", p.key, p.name, p.description);
    }
    Ok(())
}

/// Parses a simple `Nd` / `N` (days) string. `""`/`"0"` mean "disabled" (0).
fn parse_duration_days(s: &str) -> CliResult<i64> {
    let s = s.trim();
    if s.is_empty() || s == "0" {
        return Ok(0);
    }
    let trimmed = s.strip_suffix('d').unwrap_or(s);
    match trimmed.parse::<i64>() {
        Ok(n) if n >= 0 => Ok(n),
        _ => Err(CliError::validation(format!(
            "invalid duration {s:?} (use e.g. 7d or 0 to disable)"
        ))),
    }
}

async fn edit(
    db: &Option<String>,
    key: String,
    name: Option<String>,
    description: Option<String>,
    auto_archive_done_after: Option<String>,
) -> CliResult<()> {
    let key = key.to_uppercase();
    // Parse the duration before opening the store so a bad value fails fast,
    // matching Go's order of effects only for the auto-archive update (which
    // Go runs after the name/description update — but parse errors there abort
    // before any DB write of the duration). Go updates name/desc first, then
    // sets the duration. We mirror that: parse here, write both via store.
    let days = match &auto_archive_done_after {
        Some(s) => Some(parse_duration_days(s)?),
        None => None,
    };
    let store = store_open::open(db).await?;
    store
        .call(move |conn| {
            let cur = projects::fetch_by_key(conn, &key)?;
            let new_name = name.unwrap_or_else(|| cur.name.clone());
            let new_desc = description.unwrap_or_else(|| cur.description.clone());
            projects::update(
                conn,
                &cur,
                UpdateProject {
                    name: Some(new_name),
                    description: Some(new_desc),
                    archived: None,
                    auto_archive_done_after_days: None,
                },
            )?;
            if let Some(days) = days {
                let cur = projects::fetch_by_key(conn, &key)?;
                projects::update(
                    conn,
                    &cur,
                    UpdateProject {
                        auto_archive_done_after_days: Some(Some(days)),
                        ..Default::default()
                    },
                )?;
            }
            Ok(())
        })
        .await?;
    Ok(())
}

async fn set_archived(db: &Option<String>, key: String, archived: bool) -> CliResult<()> {
    let key = key.to_uppercase();
    let store = store_open::open(db).await?;
    store
        .call(move |conn| {
            let cur = projects::fetch_by_key(conn, &key)?;
            projects::update(
                conn,
                &cur,
                UpdateProject {
                    archived: Some(archived),
                    ..Default::default()
                },
            )?;
            Ok(())
        })
        .await?;
    Ok(())
}

async fn rm(db: &Option<String>, key: String, force: bool) -> CliResult<()> {
    let key = key.to_uppercase();
    let store = store_open::open(db).await?;
    store
        .call(move |conn| {
            let issues = issues::list(
                conn,
                ListOpts {
                    project: Some(&key),
                    archived: false,
                    ..Default::default()
                },
            )?;
            if !issues.is_empty() && !force {
                return Err(cliban_core::Error::validation(
                    "project",
                    &format!(
                        "project {} has {} issues; pass --force to delete",
                        key,
                        issues.len()
                    ),
                ));
            }
            let cur = projects::fetch_by_key(conn, &key)?;
            projects::delete(conn, &cur)
        })
        .await?;
    Ok(())
}
