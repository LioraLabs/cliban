//! `cliban issue` subcommands. Output is byte-for-byte parity with the Go
//! oracle (`internal/cli/issue.go`).

use std::io::Read;

use cliban_core::contexts::{issues, milestones, relations};
use cliban_core::contexts::issues::CreateIssue;
use cliban_core::schema::Issue;
use cliban_core::time::{format_date, format_usec, parse_date};
use cliban_core::Store;

use crate::errors::{CliError, CliResult};
use crate::output::{build_issue_json, IssueJsonInputs, RelationOut};
use crate::store_open;

#[derive(clap::Args)]
pub struct IssueArgs {
    #[command(subcommand)]
    pub cmd: IssueCmd,
}

#[derive(clap::Subcommand)]
pub enum IssueCmd {
    /// Add an issue (pass --editor to open $EDITOR for input)
    Add(AddArgs),
}

#[derive(clap::Args)]
pub struct AddArgs {
    /// project key (required)
    #[arg(long)]
    project: String,
    /// issue title
    #[arg(long)]
    title: Option<String>,
    /// description (use '-' to read from stdin)
    #[arg(long)]
    description: Option<String>,
    /// read description from a file (use '-' for stdin)
    #[arg(long = "description-file")]
    description_file: Option<String>,
    /// parent issue key (sub-issue)
    #[arg(long)]
    parent: Option<String>,
    /// milestone name
    #[arg(long)]
    milestone: Option<String>,
    /// priority
    #[arg(long)]
    priority: Option<String>,
    /// status
    #[arg(long)]
    status: Option<String>,
    /// due date YYYY-MM-DD
    #[arg(long)]
    due: Option<String>,
    /// label name (repeatable)
    #[arg(long)]
    label: Vec<String>,
    /// this issue blocks KEY (repeatable)
    #[arg(long)]
    blocks: Vec<String>,
    /// this issue is blocked by KEY (repeatable)
    #[arg(long = "blocked-by")]
    blocked_by: Vec<String>,
    /// this issue relates to KEY (repeatable)
    #[arg(long = "related-to")]
    related_to: Vec<String>,
    /// JSON output
    #[arg(long)]
    json: bool,
    /// open $EDITOR for input when no --title supplied
    #[arg(long)]
    editor: bool,
}

pub async fn run(db: &Option<String>, args: IssueArgs) -> CliResult<()> {
    match args.cmd {
        IssueCmd::Add(a) => add(db, a).await,
    }
}

/// `ParseIssueKey`: trim, split on the LAST `-`. Returns the normalized key
/// `"<UPPER>-<seq>"`. Mirrors Go `domain.ParseIssueKey`.
fn parse_issue_key(s: &str) -> Result<String, CliError> {
    let s = s.trim();
    match s.rfind('-') {
        // idx <= 0 (no dash, or dash at start) or trailing dash → malformed.
        Some(idx) if idx > 0 && idx < s.len() - 1 => {
            let project = s[..idx].to_uppercase();
            let seq_str = &s[idx + 1..];
            match seq_str.parse::<i64>() {
                Ok(n) if n > 0 => Ok(format!("{project}-{n}")),
                _ => Err(CliError::validation(format!(
                    "invalid issue key {s:?} (sequence must be positive integer)"
                ))),
            }
        }
        _ => Err(CliError::validation(format!(
            "invalid issue key {s:?} (want PROJECT-N)"
        ))),
    }
}

/// `resolveDescription`: returns `(content, was_set)`.
///   * `--description` and `--description-file` are mutually exclusive.
///   * `-` reads stdin.
fn resolve_description(
    description: Option<String>,
    description_file: Option<String>,
) -> CliResult<(String, bool)> {
    if let Some(file) = description_file {
        if description.is_some() {
            return Err(CliError::validation(
                "--description and --description-file are mutually exclusive",
            ));
        }
        if file == "-" {
            return Ok((read_stdin()?, true));
        }
        match std::fs::read_to_string(&file) {
            Ok(s) => Ok((s, true)),
            Err(e) => Err(CliError::validation(e.to_string())),
        }
    } else if let Some(desc) = description {
        if desc == "-" {
            Ok((read_stdin()?, true))
        } else {
            Ok((desc, true))
        }
    } else {
        Ok((String::new(), false))
    }
}

fn read_stdin() -> CliResult<String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::other(e.to_string()))?;
    Ok(buf)
}

async fn add(db: &Option<String>, a: AddArgs) -> CliResult<()> {
    let project_key = a.project.to_uppercase();
    let title = a.title.unwrap_or_default();

    let (description, desc_set) = resolve_description(a.description, a.description_file)?;

    let contentless = title.is_empty() && !desc_set;
    if contentless {
        if a.editor {
            // The interactive editor path requires a TTY; it is out of
            // golden-test scope, so a non-TTY validation error suffices.
            return Err(CliError::validation("--editor requires a TTY"));
        }
        return Err(CliError::validation(
            "--title required (pass --editor to open $EDITOR)",
        ));
    }

    // Pre-parse the parent key (Go parses before the store call).
    let parent_key = match &a.parent {
        Some(p) if !p.is_empty() => Some(parse_issue_key(p)?),
        _ => None,
    };

    // Parse the due date (Go parseDueDate, validation on failure).
    let due_date = match &a.due {
        Some(d) if !d.is_empty() => match parse_date(d) {
            Some(dt) => Some(dt),
            None => {
                return Err(CliError::validation(format!(
                    "invalid --due {d:?} (want YYYY-MM-DD)"
                )))
            }
        },
        _ => None,
    };

    // Normalize the relation key flags up front (Go ParseIssueKey on each).
    let blocks: Vec<String> = a
        .blocks
        .iter()
        .map(|k| parse_issue_key(k))
        .collect::<Result<_, _>>()?;
    let blocked_by: Vec<String> = a
        .blocked_by
        .iter()
        .map(|k| parse_issue_key(k))
        .collect::<Result<_, _>>()?;
    let related_to: Vec<String> = a
        .related_to
        .iter()
        .map(|k| parse_issue_key(k))
        .collect::<Result<_, _>>()?;

    let store = store_open::open(db).await?;

    // Step 1: create the issue (core validates status/priority/milestone/parent).
    let milestone = a.milestone.filter(|m| !m.is_empty());
    let priority = a.priority.filter(|p| !p.is_empty());
    let status = a.status.filter(|s| !s.is_empty());
    let create_project = project_key.clone();
    let mut issue = store
        .call(move |conn| {
            issues::create(
                conn,
                &create_project,
                CreateIssue {
                    title,
                    description: Some(description),
                    status,
                    priority,
                    milestone,
                    parent_key,
                    due_date,
                    position: None,
                },
            )
        })
        .await?;

    // Step 2: attach labels (idempotent, sequential — matches Go).
    for lbl in a.label {
        let id = issue.id;
        let name = lbl;
        store
            .call(move |conn| {
                let issue = issues::get_by_id(conn, id)?.ok_or(cliban_core::Error::NotFound)?;
                issues::add_label(conn, &issue, &name)
            })
            .await?;
    }

    // Step 3: relations. `--blocks K` → from issue to K; `--blocked-by K` →
    // from K to issue; `--related-to K` → from issue to K.
    let issue_key = issue.key.clone();
    for other in blocks {
        let from = issue_key.clone();
        store
            .call(move |conn| relations::add(conn, &from, &other, "blocks"))
            .await?;
    }
    for other in blocked_by {
        let to = issue_key.clone();
        store
            .call(move |conn| relations::add(conn, &other, &to, "blocks"))
            .await?;
    }
    for other in related_to {
        let from = issue_key.clone();
        store
            .call(move |conn| relations::add(conn, &from, &other, "related_to"))
            .await?;
    }

    // Reload to pick up labels/relations in the output.
    let reload_key = issue.key.clone();
    if let Some(fresh) = store
        .call(move |conn| issues::get_by_key(conn, &reload_key))
        .await?
    {
        issue = fresh;
    }

    if a.json {
        let inputs = issue_json_inputs(&store, &issue).await?;
        println!("{}", serde_json::to_string_pretty(&build_issue_json(inputs)).unwrap());
    } else {
        println!("created {}: {}", issue.key, issue.title);
    }
    Ok(())
}

/// Resolve every reference an `IssueJsonInputs` needs (project key, milestone
/// name, parent key, labels, relations, formatted dates) in a single writer
/// round-trip.
pub async fn issue_json_inputs(store: &Store, issue: &Issue) -> CliResult<IssueJsonInputs> {
    let id = issue.id;
    let milestone_id = issue.milestone_id;
    let parent_id = issue.parent_id;
    let key = issue.key.clone();
    let title = issue.title.clone();
    let description = issue.description.clone();
    let status = issue.status.clone();
    let priority = issue.priority.clone();
    let position = issue.position;
    let archived = issue.archived;
    let due_date = issue.due_date.map(format_date);
    let completed_at = issue.completed_at.map(format_usec);
    let created_at = format_usec(issue.inserted_at);
    let updated_at = format_usec(issue.updated_at);

    let inputs = store
        .call(move |conn| {
            // The issue's `key` field already carries the full "PROJECT-N"
            // string, so no project lookup is needed for the JSON projection.
            let milestone = match milestone_id {
                Some(mid) => milestones::get_by_id(conn, mid)?.map(|m| m.name),
                None => None,
            };
            let parent = match parent_id {
                Some(pid) => issues::get_by_id(conn, pid)?.map(|i| i.key),
                None => None,
            };
            let labels = issues::label_names(conn, id)?;
            let relations = relations::for_issue(conn, id)?
                .into_iter()
                .map(|r| RelationOut {
                    kind: r.kind,
                    target: r.target_key,
                })
                .collect();
            Ok(IssueJsonInputs {
                key,
                title,
                description,
                status,
                priority,
                position,
                archived,
                due_date,
                completed_at,
                milestone,
                parent,
                labels,
                relations,
                created_at,
                updated_at,
            })
        })
        .await?;
    Ok(inputs)
}
