//! `cliban milestone` subcommands. Output is byte-for-byte parity with the Go
//! oracle (`internal/cli/milestone.go`).

use chrono::NaiveDate;
use serde_json::{json, Map, Value};

use cliban_core::contexts::issues::ListOpts;
use cliban_core::contexts::milestones::{CreateMilestone, UpdateMilestone};
use cliban_core::contexts::{issues, milestones, projects};
use cliban_core::schema::Milestone;
use cliban_core::time::{format_date, format_usec, parse_date};
use cliban_core::Store;

use crate::cmd::issue::issue_json_inputs;
use crate::errors::{CliError, CliResult};
use crate::output::{build_issue_json, write_issue_table, IssueRow};
use crate::store_open;

#[derive(clap::Args)]
pub struct MilestoneArgs {
    #[command(subcommand)]
    pub cmd: MilestoneCmd,
}

#[derive(clap::Subcommand)]
pub enum MilestoneCmd {
    /// Add a milestone
    Add {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "description-file")]
        description_file: Option<String>,
        /// target date YYYY-MM-DD
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List milestones
    Ls {
        #[arg(long)]
        project: String,
        /// filter by status
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show a milestone (accepts positional NAME or --name)
    Show {
        /// milestone name (or pass via --name)
        name: Option<String>,
        #[arg(long)]
        project: String,
        #[arg(long = "name")]
        name_flag: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long = "with-issues")]
        with_issues: bool,
    },
    /// Edit a milestone
    Edit {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// new name
        #[arg(long)]
        rename: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "description-file")]
        description_file: Option<String>,
        /// new status (open|completed|cancelled)
        #[arg(long)]
        status: Option<String>,
        /// new target date YYYY-MM-DD
        #[arg(long)]
        target: Option<String>,
        /// clear target date
        #[arg(long = "clear-target")]
        clear_target: bool,
    },
    /// Delete a milestone
    Rm {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
    },
}

pub async fn run(db: &Option<String>, args: MilestoneArgs) -> CliResult<()> {
    match args.cmd {
        MilestoneCmd::Add {
            project,
            name,
            description,
            description_file,
            target,
            json,
        } => {
            add(
                db,
                project,
                name,
                description,
                description_file,
                target,
                json,
            )
            .await
        }
        MilestoneCmd::Ls {
            project,
            status,
            json,
        } => ls(db, project, status, json).await,
        MilestoneCmd::Show {
            name,
            project,
            name_flag,
            json,
            with_issues,
        } => show(db, name, project, name_flag, json, with_issues).await,
        MilestoneCmd::Edit {
            project,
            name,
            rename,
            description,
            description_file,
            status,
            target,
            clear_target,
        } => {
            edit(
                db,
                project,
                name,
                rename,
                description,
                description_file,
                status,
                target,
                clear_target,
            )
            .await
        }
        MilestoneCmd::Rm { project, name } => rm(db, project, name).await,
    }
}

/// `parseTarget`: empty → None; otherwise parse `YYYY-MM-DD`. A parse failure is
/// a plain (exit-3) error in Go (NOT wrapped in ErrValidation).
fn parse_target(s: &str) -> CliResult<Option<NaiveDate>> {
    if s.is_empty() {
        return Ok(None);
    }
    match parse_date(s) {
        Some(d) => Ok(Some(d)),
        None => Err(CliError::other(format!(
            "invalid --target {s:?} (want YYYY-MM-DD)"
        ))),
    }
}

/// `resolveDescription`: returns `(content, was_set)`. `--description` and
/// `--description-file` are mutually exclusive; `-` reads stdin.
fn resolve_description(
    description: Option<String>,
    description_file: Option<String>,
) -> CliResult<(String, bool)> {
    use std::io::Read;
    if let Some(file) = description_file {
        if description.is_some() {
            return Err(CliError::validation(
                "--description and --description-file are mutually exclusive",
            ));
        }
        if file == "-" {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| CliError::other(e.to_string()))?;
            return Ok((buf, true));
        }
        match std::fs::read_to_string(&file) {
            Ok(s) => Ok((s, true)),
            Err(e) => Err(CliError::validation(e.to_string())),
        }
    } else if let Some(desc) = description {
        if desc == "-" {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| CliError::other(e.to_string()))?;
            Ok((buf, true))
        } else {
            Ok((desc, true))
        }
    } else {
        Ok((String::new(), false))
    }
}

/// Count non-archived issues with the given milestone name in the given project
/// (mirrors `milestoneToJSON`'s issue_count).
async fn issue_count(store: &Store, project: String, milestone: String) -> CliResult<i64> {
    let n = store
        .call(move |conn| {
            let list = issues::list(
                conn,
                ListOpts {
                    project: Some(&project),
                    milestone: Some(&milestone),
                    archived: false,
                    ..Default::default()
                },
            )?;
            Ok(list.len() as i64)
        })
        .await?;
    Ok(n)
}

fn milestone_json(m: &Milestone, project: &str, count: i64) -> Value {
    crate::output::build_milestone_json(
        &m.name,
        Some(project.to_string()),
        &m.description,
        m.target_date.map(format_date),
        &m.status,
        &format_usec(m.inserted_at),
        &format_usec(m.updated_at),
        count,
    )
}

async fn add(
    db: &Option<String>,
    project: String,
    name: String,
    description: Option<String>,
    description_file: Option<String>,
    target: Option<String>,
    json: bool,
) -> CliResult<()> {
    let project_key = project.to_uppercase();
    let target_date = parse_target(target.as_deref().unwrap_or(""))?;
    let (description, _set) = resolve_description(description, description_file)?;

    let store = store_open::open(db).await?;
    let create_project = project_key.clone();
    let create_name = name.clone();
    let m = store
        .call(move |conn| {
            milestones::create(
                conn,
                CreateMilestone {
                    project: create_project,
                    name: create_name,
                    description: Some(description),
                    target_date,
                    status: None,
                },
            )
        })
        .await?;

    if json {
        let count = issue_count(&store, project_key.clone(), m.name.clone()).await?;
        let v = milestone_json(&m, &project_key, count);
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        println!("created milestone {} in {}", m.name, project_key);
    }
    Ok(())
}

async fn ls(
    db: &Option<String>,
    project: String,
    status: Option<String>,
    json: bool,
) -> CliResult<()> {
    let project_key = project.to_uppercase();
    let store = store_open::open(db).await?;
    let list_key = project_key.clone();
    let mut ms = store
        .call(move |conn| milestones::list(conn, Some(&list_key)))
        .await?;
    if let Some(s) = status.filter(|s| !s.is_empty()) {
        ms.retain(|m| m.status == s);
    }

    if json {
        for m in &ms {
            let count = issue_count(&store, project_key.clone(), m.name.clone()).await?;
            let v = milestone_json(m, &project_key, count);
            println!("{}", serde_json::to_string(&v).unwrap());
        }
    } else {
        for m in &ms {
            let tgt = m
                .target_date
                .map(format_date)
                .unwrap_or_else(|| "-".to_string());
            println!("{:<15} {:<10} {}", m.name, m.status, tgt);
        }
    }
    Ok(())
}

async fn show(
    db: &Option<String>,
    name: Option<String>,
    project: String,
    name_flag: Option<String>,
    json: bool,
    with_issues: bool,
) -> CliResult<()> {
    // name = positional XOR --name (equal is ok); none → validation error.
    let mut resolved = name_flag.clone().unwrap_or_default();
    if let Some(pos) = &name {
        if !resolved.is_empty() && &resolved != pos {
            return Err(CliError::validation(
                "pass NAME positionally OR via --name, not both",
            ));
        }
        resolved = pos.clone();
    }
    if resolved.is_empty() {
        return Err(CliError::validation(
            "milestone name required (positional or --name)",
        ));
    }
    if project.is_empty() {
        return Err(CliError::validation("--project is required"));
    }
    let project_key = project.to_uppercase();

    let store = store_open::open(db).await?;
    let get_key = project_key.clone();
    let get_name = resolved.clone();
    let m = store
        .call(move |conn| milestones::get(conn, &get_key, &get_name))
        .await?
        .ok_or(cliban_core::Error::NotFound)?;

    // Resolve issue list for the milestone (non-archived, this project).
    let list_key = project_key.clone();
    let list_name = resolved.clone();
    let issue_list = store
        .call(move |conn| {
            issues::list(
                conn,
                ListOpts {
                    project: Some(&list_key),
                    milestone: Some(&list_name),
                    archived: false,
                    ..Default::default()
                },
            )
        })
        .await?;
    let count = issue_list.len() as i64;

    if json {
        // Build alpha-ordered map inline so `issues` lands between issue_count
        // and name (matches Go's map[string]any alphabetical serialization).
        let mut map = Map::new();
        map.insert("created_at".into(), json!(format_usec(m.inserted_at)));
        map.insert("description".into(), json!(m.description));
        map.insert("issue_count".into(), json!(count));
        if with_issues {
            let mut arr = Vec::with_capacity(issue_list.len());
            for i in &issue_list {
                let inputs = issue_json_inputs(&store, i).await?;
                arr.push(build_issue_json(inputs));
            }
            map.insert("issues".into(), Value::Array(arr));
        }
        map.insert("name".into(), json!(m.name));
        map.insert("project".into(), json!(project_key));
        map.insert("status".into(), json!(m.status));
        map.insert(
            "target_date".into(),
            match m.target_date.map(format_date) {
                Some(s) => json!(s),
                None => Value::Null,
            },
        );
        map.insert("updated_at".into(), json!(format_usec(m.updated_at)));
        println!(
            "{}",
            serde_json::to_string_pretty(&Value::Object(map)).unwrap()
        );
        return Ok(());
    }

    let tgt = m
        .target_date
        .map(format_date)
        .unwrap_or_else(|| "-".to_string());
    print!(
        "{} — {}\nstatus:  {}\ntarget:  {}\nissues:  {}\n{}\n",
        m.name, project_key, m.status, tgt, count, m.description
    );
    if with_issues {
        println!();
        let mut rows = Vec::with_capacity(issue_list.len());
        for i in &issue_list {
            let (ms_name, parent_key) = resolve_refs(&store, i).await?;
            rows.push(IssueRow {
                key: i.key.clone(),
                title: i.title.clone(),
                status: i.status.clone(),
                priority: i.priority.clone(),
                milestone: ms_name,
                parent: parent_key,
            });
        }
        print!("{}", write_issue_table(&rows));
    }
    Ok(())
}

/// Resolve milestone name + parent key for an issue (empty when unset).
async fn resolve_refs(
    store: &Store,
    issue: &cliban_core::schema::Issue,
) -> CliResult<(String, String)> {
    let milestone_id = issue.milestone_id;
    let parent_id = issue.parent_id;
    let pair = store
        .call(move |conn| {
            let milestone = match milestone_id {
                Some(mid) => milestones::get_by_id(conn, mid)?.map(|m| m.name),
                None => None,
            };
            let parent = match parent_id {
                Some(pid) => issues::get_by_id(conn, pid)?.map(|i| i.key),
                None => None,
            };
            Ok((milestone.unwrap_or_default(), parent.unwrap_or_default()))
        })
        .await?;
    Ok(pair)
}

#[allow(clippy::too_many_arguments)]
async fn edit(
    db: &Option<String>,
    project: String,
    name: String,
    rename: Option<String>,
    description: Option<String>,
    description_file: Option<String>,
    status: Option<String>,
    target: Option<String>,
    clear_target: bool,
) -> CliResult<()> {
    let project_key = project.to_uppercase();
    let (desc_content, desc_set) = resolve_description(description, description_file)?;

    let mut params = UpdateMilestone::default();
    if let Some(r) = rename {
        params.name = Some(r);
    }
    if desc_set {
        params.description = Some(desc_content);
    }
    if let Some(s) = status {
        params.status = Some(s);
    }
    if clear_target {
        params.target_date = Some(None);
    } else if let Some(t) = &target {
        let d = parse_target(t)?;
        params.target_date = Some(d);
    }

    let store = store_open::open(db).await?;
    store
        .call(move |conn| {
            let cur =
                milestones::get(conn, &project_key, &name)?.ok_or(cliban_core::Error::NotFound)?;
            milestones::update(conn, &cur, params)?;
            Ok(())
        })
        .await?;
    Ok(())
}

async fn rm(db: &Option<String>, project: String, name: String) -> CliResult<()> {
    let project_key = project.to_uppercase();
    let store = store_open::open(db).await?;
    // Go's DeleteMilestone resolves the milestone first (GetMilestone) and
    // returns an error (exit 1) when project or milestone is missing; replicate
    // that before the raw DELETE.
    store
        .call(move |conn| {
            let p = projects::get_by_key(conn, &project_key)?
                .ok_or(cliban_core::Error::ProjectNotFound)?;
            if milestones::get(conn, &project_key, &name)?.is_none() {
                return Err(cliban_core::Error::NotFound);
            }
            conn.execute(
                "DELETE FROM milestones WHERE project_id = ?1 AND name = ?2",
                rusqlite::params![p.id, name],
            )?;
            Ok(())
        })
        .await?;
    Ok(())
}
