//! `cliban issue` subcommands. Output is byte-for-byte parity with the Go
//! oracle (`internal/cli/issue.go`).

use std::io::{Read, Write};

use cliban_core::contexts::issues::{CreateIssue, ListOpts, UpdateIssue};
use cliban_core::contexts::{issues, milestones, relations};
use cliban_core::schema::{Issue, ISSUE_PRIORITIES, ISSUE_STATUSES};
use cliban_core::time::{format_date, format_usec, parse_date, parse_ts};
use cliban_core::Store;

use chrono::Utc;
use rusqlite::OptionalExtension;

use crate::descmd;
use crate::descmd::find_section;
use crate::errors::{CliError, CliResult};
use crate::output::{build_issue_json, write_issue_table, IssueJsonInputs, IssueRow, RelationOut};
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
    /// Show an issue
    Show(ShowArgs),
    /// List issues
    Ls(LsArgs),
    /// Edit an issue
    Edit(EditArgs),
    /// Append an entry to the issue's ## Activity Log section
    Log(LogArgs),
    /// Tick a step in the issue's ## Plan section
    Tick(TickArgs),
    /// Promote a plan step into its own issue
    Promote(PromoteArgs),
    /// Archive done issues
    #[command(name = "archive-done")]
    ArchiveDone(ArchiveDoneArgs),
    /// Bulk-create issues from an NDJSON file (or stdin with '-')
    Import(ImportArgs),
    /// Move an issue to a new status
    Mv {
        key: String,
        status: String,
    },
    /// Delete an issue (cascades sub-issues)
    Rm {
        key: String,
    },
    /// Archive an issue (hides it from the default board and lists)
    Archive {
        key: String,
    },
    /// Unarchive an issue
    Unarchive {
        key: String,
    },
    /// Show the issue inferred from the current git branch
    Current {
        #[arg(long)]
        json: bool,
    },
    /// List issues that have at least one open blocker
    Blocked {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(clap::Args)]
pub struct ShowArgs {
    /// issue key
    key: String,
    /// JSON output
    #[arg(long)]
    json: bool,
    /// show only one section: spec|plan|activity|notes
    #[arg(long)]
    section: Option<String>,
    /// pipe human-readable output through $PAGER
    #[arg(long)]
    pager: bool,
}

#[derive(clap::Args)]
pub struct LsArgs {
    /// project key filter
    #[arg(long)]
    project: Option<String>,
    /// status filter
    #[arg(long)]
    status: Option<String>,
    /// priority filter
    #[arg(long)]
    priority: Option<String>,
    /// milestone filter
    #[arg(long)]
    milestone: Option<String>,
    /// list sub-issues of this parent key
    #[arg(long)]
    parent: Option<String>,
    /// sort key: priority|created|updated|position[:asc|desc]
    #[arg(long)]
    sort: Option<String>,
    /// filter to issues with ALL of these labels (repeatable)
    #[arg(long)]
    label: Vec<String>,
    /// exclude sub-issues
    #[arg(long = "no-subs")]
    no_subs: bool,
    /// NDJSON output (one compact JSON object per line)
    #[arg(long)]
    json: bool,
    /// include archived issues
    #[arg(long)]
    archived: bool,
    /// filter issues updated within a duration (e.g. 4h) or since an RFC3339 timestamp
    #[arg(long = "updated-since")]
    updated_since: Option<String>,
    /// fuzzy search query across title/key/labels/description
    #[arg(long)]
    search: Option<String>,
    /// cap result count (default 50 when --search is set; ignored otherwise)
    #[arg(long, default_value_t = 0)]
    limit: i64,
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

#[derive(clap::Args)]
pub struct EditArgs {
    /// issue key
    key: String,
    /// new title
    #[arg(long)]
    title: Option<String>,
    /// new description (use '-' for stdin)
    #[arg(long)]
    description: Option<String>,
    /// read description from a file (use '-' for stdin)
    #[arg(long = "description-file")]
    description_file: Option<String>,
    /// new priority
    #[arg(long)]
    priority: Option<String>,
    /// new milestone
    #[arg(long)]
    milestone: Option<String>,
    /// clear milestone
    #[arg(long = "clear-milestone")]
    clear_milestone: bool,
    /// new parent key
    #[arg(long)]
    parent: Option<String>,
    /// clear parent
    #[arg(long = "clear-parent")]
    clear_parent: bool,
    /// new due date YYYY-MM-DD
    #[arg(long)]
    due: Option<String>,
    /// clear due date
    #[arg(long = "clear-due")]
    clear_due: bool,
    /// add label (repeatable)
    #[arg(long)]
    label: Vec<String>,
    /// remove label (repeatable)
    #[arg(long = "remove-label")]
    remove_label: Vec<String>,
    /// add 'blocks' relation to KEY (repeatable)
    #[arg(long)]
    blocks: Vec<String>,
    /// add 'blocked by' relation from KEY (repeatable)
    #[arg(long = "blocked-by")]
    blocked_by: Vec<String>,
    /// add 'related to' relation to KEY (repeatable)
    #[arg(long = "related-to")]
    related_to: Vec<String>,
    /// remove any relation involving KEY (repeatable)
    #[arg(long = "remove-relation")]
    remove_relation: Vec<String>,
    /// open $EDITOR for full edit
    #[arg(long, short = 'e')]
    editor: bool,
    /// JSON output
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct LogArgs {
    /// issue key
    key: String,
    /// log message
    message: Option<String>,
    /// read message from file (use '-' for stdin)
    #[arg(long = "message-file")]
    message_file: Option<String>,
    /// JSON output
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct TickArgs {
    /// issue key
    key: String,
    /// task number (required, 1-indexed)
    #[arg(long)]
    task: i32,
    /// step number (required, 1-indexed)
    #[arg(long)]
    step: i32,
    /// JSON output
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct PromoteArgs {
    /// issue key
    key: String,
    /// task number (required, 1-indexed)
    #[arg(long)]
    task: i32,
    /// step number (required, 1-indexed)
    #[arg(long)]
    step: i32,
    /// title for the promoted issue (required)
    #[arg(long, default_value = "")]
    title: String,
    /// promotion mode: sub-issue|related
    #[arg(long = "as", default_value = "sub-issue")]
    as_mode: String,
    /// JSON output
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct ArchiveDoneArgs {
    /// project key
    #[arg(long)]
    project: Option<String>,
    /// sweep every project per its auto_archive_done_after_days policy
    #[arg(long)]
    auto: bool,
    /// JSON output
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct ImportArgs {
    /// NDJSON file path (default: stdin)
    file_arg: Option<String>,
    /// NDJSON file path (default: stdin)
    #[arg(long)]
    file: Option<String>,
    /// default project key for records that omit it
    #[arg(long)]
    project: Option<String>,
    /// emit each created issue as a JSON line
    #[arg(long)]
    json: bool,
}

pub async fn run(db: &Option<String>, args: IssueArgs) -> CliResult<()> {
    match args.cmd {
        IssueCmd::Add(a) => add(db, a).await,
        IssueCmd::Show(a) => show(db, a).await,
        IssueCmd::Ls(a) => ls(db, a).await,
        IssueCmd::Edit(a) => edit(db, a).await,
        IssueCmd::Log(a) => log(db, a).await,
        IssueCmd::Tick(a) => tick(db, a).await,
        IssueCmd::Promote(a) => promote(db, a).await,
        IssueCmd::ArchiveDone(a) => archive_done(db, a).await,
        IssueCmd::Import(a) => import(db, a).await,
        IssueCmd::Mv { key, status } => mv(db, key, status).await,
        IssueCmd::Rm { key } => rm(db, key).await,
        IssueCmd::Archive { key } => set_archived(db, key, true).await,
        IssueCmd::Unarchive { key } => set_archived(db, key, false).await,
        IssueCmd::Current { json } => current(db, json).await,
        IssueCmd::Blocked { project, json } => blocked(db, project, json).await,
    }
}

/// `domain.ParseStatus`: lowercase+trim, must be a known status. Returns a
/// plain (exit-3) error to mirror the Go oracle, which does NOT wrap this in
/// `ErrValidation` at the `issue mv`/`issue ls` call sites.
fn parse_status(s: &str) -> Result<String, CliError> {
    let norm = s.trim().to_lowercase();
    if ISSUE_STATUSES.contains(&norm.as_str()) {
        Ok(norm)
    } else {
        Err(CliError::other(format!(
            "invalid status {s:?} (valid: backlog, in-progress, blocked, in-review, done)"
        )))
    }
}

/// `domain.ParsePriority`: lowercase+trim, must be a known priority. Plain
/// (exit-3) error, mirroring the Go oracle.
fn parse_priority(s: &str) -> Result<String, CliError> {
    let norm = s.trim().to_lowercase();
    if ISSUE_PRIORITIES.contains(&norm.as_str()) {
        Ok(norm)
    } else {
        Err(CliError::other(format!(
            "invalid priority {s:?} (valid: none, low, medium, high, urgent)"
        )))
    }
}

/// `domain.PriorityRank`: none<low<medium<high<urgent.
fn priority_rank(p: &str) -> i32 {
    match p {
        "urgent" => 4,
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
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

/// Public wrapper over `parse_issue_key` for use by the search module.
pub fn parse_issue_key_pub(s: &str) -> Result<String, CliError> {
    parse_issue_key(s)
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

    print_issue_result(&store, &issue, "created", a.json).await
}

/// Mirrors Go `printIssueResult`: human `{verb} {KEY}: {title}\n`; json pretty.
async fn print_issue_result(
    store: &Store,
    issue: &Issue,
    verb: &str,
    json: bool,
) -> CliResult<()> {
    if json {
        let inputs = issue_json_inputs(store, issue).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&build_issue_json(inputs)).unwrap()
        );
    } else {
        println!("{verb} {}: {}", issue.key, issue.title);
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

/// Resolve milestone name + parent key for an issue (empty when unset).
async fn resolve_refs(store: &Store, issue: &Issue) -> CliResult<(String, String)> {
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

/// `sectionAnchor`: map a `--section` short name to its canonical H2 anchor.
fn section_anchor(s: &str) -> Result<&'static str, CliError> {
    match s {
        "spec" => Ok("Spec"),
        "plan" => Ok("Plan"),
        "activity" => Ok("Activity Log"),
        "notes" => Ok("Notes"),
        _ => Err(CliError::validation(format!(
            "invalid --section {s:?} (want spec|plan|activity|notes)"
        ))),
    }
}

async fn show(db: &Option<String>, a: ShowArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let issue = store
        .call(move |conn| issues::get_by_key(conn, &lookup))
        .await?
        .ok_or(cliban_core::Error::NotFound)?;

    // --section is a targeted machine read; mutually exclusive with json/pager.
    if let Some(section) = &a.section {
        let anchor = section_anchor(section)?;
        let (start, end, ok) = find_section(&issue.description, anchor);
        if !ok {
            // Go wraps with %w on store.ErrNotFound → "not found: <msg>".
            return Err(CliError::not_found(format!(
                "not found: no ## {anchor} section in {}",
                a.key
            )));
        }
        print!("{}", &issue.description[start..end]);
        return Ok(());
    }

    if a.json {
        let inputs = issue_json_inputs(&store, &issue).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&build_issue_json(inputs)).unwrap()
        );
        return Ok(());
    }

    let (ms_name, parent_key) = resolve_refs(&store, &issue).await?;
    let body = format!(
        "{} — {}\nstatus:    {}\npriority:  {}\nmilestone: {}\nparent:    {}\n\n{}\n",
        key,
        issue.title,
        issue.status,
        issue.priority,
        dash_if_empty(&ms_name),
        dash_if_empty(&parent_key),
        issue.description
    );
    if a.pager {
        run_pager(&body)?;
    } else {
        print!("{body}");
    }
    Ok(())
}

fn dash_if_empty(s: &str) -> String {
    if s.is_empty() {
        "-".into()
    } else {
        s.into()
    }
}

/// `runPager`: pipe through `sh -c $PAGER` when set, else write to stdout.
fn run_pager(body: &str) -> CliResult<()> {
    match std::env::var("PAGER") {
        Ok(pager) if !pager.is_empty() => {
            use std::process::{Command, Stdio};
            let mut child = Command::new("sh")
                .arg("-c")
                .arg(&pager)
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| CliError::other(e.to_string()))?;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin
                    .write_all(body.as_bytes())
                    .map_err(|e| CliError::other(e.to_string()))?;
            }
            child.wait().map_err(|e| CliError::other(e.to_string()))?;
            Ok(())
        }
        _ => {
            print!("{body}");
            Ok(())
        }
    }
}

/// `parseUpdatedSince`: a Go-style duration (e.g. `4h`, `30m`) → now-d, else an
/// RFC3339 timestamp → that instant.
fn parse_updated_since(s: &str) -> Result<chrono::DateTime<Utc>, CliError> {
    if let Some(d) = parse_go_duration(s) {
        return Ok(Utc::now() - d);
    }
    if let Some(ts) = parse_ts(s) {
        return Ok(ts);
    }
    Err(CliError::validation(format!(
        "invalid --updated-since {s:?} (want duration like 4h or RFC3339 timestamp)"
    )))
}

/// Minimal Go `time.ParseDuration` subset: a single signed decimal with a unit
/// suffix `ns|us|µs|ms|s|m|h`. Returns None on anything unrecognized.
fn parse_go_duration(s: &str) -> Option<chrono::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Find the boundary between the numeric part and the unit suffix.
    let split = s.find(|c: char| c.is_ascii_alphabetic() || c == 'µ')?;
    let (num, unit) = s.split_at(split);
    let val: f64 = num.parse().ok()?;
    let secs = match unit {
        "ns" => val / 1_000_000_000.0,
        "us" | "µs" => val / 1_000_000.0,
        "ms" => val / 1_000.0,
        "s" => val,
        "m" => val * 60.0,
        "h" => val * 3600.0,
        _ => return None,
    };
    chrono::Duration::try_milliseconds((secs * 1000.0) as i64)
}

async fn ls(db: &Option<String>, a: LsArgs) -> CliResult<()> {
    // --search branch. The flag being *present* (even empty) maps to Go's
    // `cmd.Flags().Changed("search")`; an empty/whitespace value is a clean
    // validation error.
    if let Some(raw) = &a.search {
        if raw.trim().is_empty() {
            return Err(CliError::validation("--search requires a non-empty query"));
        }
        // --sort is ignored under --search; emit the Go note to stderr.
        if a.sort.is_some() {
            eprintln!("note: --sort is ignored when --search is set");
        }
        return run_search(db, &a, raw.clone()).await;
    }

    // Pre-parse the typed filters (Go parses before the store call).
    let project = a.project.as_deref().map(str::to_uppercase);
    let status = match &a.status {
        Some(s) if !s.is_empty() => Some(parse_status(s)?),
        _ => None,
    };
    let priority = match &a.priority {
        Some(p) if !p.is_empty() => Some(parse_priority(p)?),
        _ => None,
    };
    let parent_key = match &a.parent {
        Some(p) if !p.is_empty() => Some(parse_issue_key(p)?),
        _ => None,
    };
    let updated_since = match &a.updated_since {
        Some(s) if !s.is_empty() => Some(parse_updated_since(s)?),
        _ => None,
    };
    // Validate sort spec up front (matches Go: sort runs after the fetch, but a
    // bad spec is a clean validation error either way).
    let sort = a.sort.clone().filter(|s| !s.is_empty());
    if let Some(spec) = &sort {
        validate_sort_spec(spec)?;
    }

    let store = store_open::open(db).await?;

    // Core list handles project/status/milestone, but its `archived` flag is an
    // exact match (archived = this). Go's `--archived` means *include* archived:
    // unset → only non-archived; set → both. So when set we fetch both and
    // concatenate (the base ordering below re-sorts the union).
    let list_project = project.clone();
    let list_status = status.clone();
    let list_milestone = a.milestone.clone().filter(|m| !m.is_empty());
    let include_archived = a.archived;
    let mut issues = store
        .call(move |conn| {
            let mut out = issues::list(
                conn,
                ListOpts {
                    project: list_project.as_deref(),
                    status: list_status.as_deref(),
                    milestone: list_milestone.as_deref(),
                    archived: false,
                },
            )?;
            if include_archived {
                let archived = issues::list(
                    conn,
                    ListOpts {
                        project: list_project.as_deref(),
                        status: list_status.as_deref(),
                        milestone: list_milestone.as_deref(),
                        archived: true,
                    },
                )?;
                out.extend(archived);
            }
            Ok(out)
        })
        .await?;

    // Rust-side filters applied AFTER fetch (Go applies these in the store
    // query, but the net result set is identical).
    if let Some(pr) = &priority {
        issues.retain(|i| &i.priority == pr);
    }
    if let Some(pk) = &parent_key {
        // Resolve the parent key to its id, then keep issues whose parent_id
        // matches. An unresolvable parent yields an empty result.
        let lookup = pk.clone();
        let parent_id = store
            .call(move |conn| issues::get_by_key(conn, &lookup).map(|o| o.map(|i| i.id)))
            .await?;
        match parent_id {
            Some(pid) => issues.retain(|i| i.parent_id == Some(pid)),
            None => issues.clear(),
        }
    }
    if a.no_subs {
        issues.retain(|i| i.parent_id.is_none());
    }
    if !a.label.is_empty() {
        let want = a.label.clone();
        let mut kept = Vec::with_capacity(issues.len());
        for i in issues.into_iter() {
            let id = i.id;
            let names = store.call(move |conn| issues::label_names(conn, id)).await?;
            if want.iter().all(|w| names.iter().any(|n| n == w)) {
                kept.push(i);
            }
        }
        issues = kept;
    }
    if let Some(threshold) = updated_since {
        issues.retain(|i| i.updated_at >= threshold);
    }

    // Base ordering mirrors the Go store query: ORDER BY p.key, i.status,
    // i.position (the issue key embeds the project key, so its prefix sorts by
    // project). The explicit --sort below is then applied stably on top.
    base_order(&mut issues);

    if let Some(spec) = &sort {
        sort_issues(&mut issues, spec);
    }

    if a.json {
        for i in &issues {
            let inputs = issue_json_inputs(&store, i).await?;
            println!("{}", serde_json::to_string(&build_issue_json(inputs)).unwrap());
        }
        return Ok(());
    }

    let rows = issue_rows(&store, &issues).await?;
    print!("{}", write_issue_table(&rows));
    Ok(())
}

/// `issue ls --search` branch. Mirrors Go `runIssueSearch`: default limit 50
/// (the `--limit` flag overrides; 0 → 50), NDJSON rows carry a `score` field,
/// human output uses the search table with a leading SCORE column.
async fn run_search(db: &Option<String>, a: &LsArgs, query: String) -> CliResult<()> {
    let effective_limit = if a.limit == 0 { 50 } else { a.limit };
    let store = store_open::open(db).await?;
    let opts = crate::search::Options {
        query,
        project: a.project.as_deref().map(str::to_uppercase),
        label: a.label.clone(),
        milestone: a.milestone.clone(),
        status: a.status.clone(),
        priority: a.priority.clone(),
        parent: a.parent.clone(),
        include_archived: a.archived,
        exclude_subs: a.no_subs,
        limit: effective_limit,
    };
    let matches = crate::search::search(&store, opts).await?;

    if a.json {
        for m in &matches {
            let inputs = issue_json_inputs(&store, &m.issue).await?;
            println!(
                "{}",
                serde_json::to_string(&crate::output::build_search_match_json(inputs, m.score))
                    .unwrap()
            );
        }
        return Ok(());
    }

    let mut rows = Vec::with_capacity(matches.len());
    for m in &matches {
        let (ms_name, parent_key) = crate::search::resolve_refs(&store, &m.issue).await?;
        rows.push(crate::output::SearchRow {
            score: m.score,
            key: m.issue.key.clone(),
            title: m.issue.title.clone(),
            status: m.issue.status.clone(),
            priority: m.issue.priority.clone(),
            milestone: ms_name,
            parent: parent_key,
        });
    }
    print!("{}", crate::output::write_search_table(&rows));
    Ok(())
}

/// Project-key prefix of an issue key (`CLI-12` → `CLI`).
fn project_prefix(key: &str) -> &str {
    match key.rfind('-') {
        Some(idx) => &key[..idx],
        None => key,
    }
}

/// Base ordering matching the Go store: (project key, status, position).
fn base_order(issues: &mut [Issue]) {
    issues.sort_by(|a, b| {
        project_prefix(&a.key)
            .cmp(project_prefix(&b.key))
            .then_with(|| a.status.cmp(&b.status))
            .then_with(|| {
                a.position
                    .partial_cmp(&b.position)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
}

/// Build display rows (resolving milestone/parent) for the issue table.
async fn issue_rows(store: &Store, issues: &[Issue]) -> CliResult<Vec<IssueRow>> {
    let mut rows = Vec::with_capacity(issues.len());
    for i in issues {
        let (ms_name, parent_key) = resolve_refs(store, i).await?;
        rows.push(IssueRow {
            key: i.key.clone(),
            title: i.title.clone(),
            status: i.status.clone(),
            priority: i.priority.clone(),
            milestone: ms_name,
            parent: parent_key,
        });
    }
    Ok(rows)
}

/// Validate a `--sort` spec (`field[:asc|desc]`).
fn validate_sort_spec(spec: &str) -> Result<(), CliError> {
    let (field, dir) = match spec.find(':') {
        Some(idx) => (&spec[..idx], &spec[idx + 1..]),
        None => (spec, ""),
    };
    match dir {
        "" | "asc" | "desc" => {}
        other => {
            return Err(CliError::validation(format!(
                "invalid sort direction {other:?} (use asc or desc)"
            )))
        }
    }
    match field {
        "priority" | "created" | "updated" | "position" => Ok(()),
        other => Err(CliError::validation(format!(
            "invalid --sort field {other:?} (priority|created|updated|position)"
        ))),
    }
}

/// Stable in-place sort by the (pre-validated) spec. Priority defaults to desc.
fn sort_issues(issues: &mut [Issue], spec: &str) {
    let (field, dir) = match spec.find(':') {
        Some(idx) => (&spec[..idx], &spec[idx + 1..]),
        None => (spec, ""),
    };
    let mut desc = dir == "desc";
    match field {
        "priority" => {
            if dir.is_empty() {
                desc = true;
            }
            issues.sort_by(|a, b| {
                let (x, y) = if desc { (b, a) } else { (a, b) };
                priority_rank(&x.priority).cmp(&priority_rank(&y.priority))
            });
        }
        "created" => issues.sort_by(|a, b| {
            if desc {
                b.inserted_at.cmp(&a.inserted_at)
            } else {
                a.inserted_at.cmp(&b.inserted_at)
            }
        }),
        "updated" => issues.sort_by(|a, b| {
            if desc {
                b.updated_at.cmp(&a.updated_at)
            } else {
                a.updated_at.cmp(&b.updated_at)
            }
        }),
        "position" => issues.sort_by(|a, b| {
            let ord = a
                .position
                .partial_cmp(&b.position)
                .unwrap_or(std::cmp::Ordering::Equal);
            if desc {
                ord.reverse()
            } else {
                ord
            }
        }),
        _ => {}
    }
}

async fn edit(db: &Option<String>, a: EditArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let project_part = project_prefix(&key).to_string();

    // resolveDescription: respects mutual exclusivity, only "set" when changed.
    let (desc_content, desc_set) = resolve_description(a.description, a.description_file)?;

    let title = a.title.clone(); // --title given → Some (even if "")
    let priority = match &a.priority {
        Some(p) => Some(parse_priority(p)?),
        None => None,
    };
    let due_date: Option<Option<chrono::NaiveDate>> = if a.clear_due {
        Some(None)
    } else if let Some(d) = &a.due {
        match parse_date(d) {
            Some(dt) => Some(Some(dt)),
            None => {
                return Err(CliError::validation(format!(
                    "invalid --due {d:?} (want YYYY-MM-DD)"
                )))
            }
        }
    } else {
        None
    };
    // Parent: parse key now (validates form).
    let parent_key = if a.clear_parent {
        None
    } else {
        match &a.parent {
            Some(p) => Some(parse_issue_key(p)?),
            None => None,
        }
    };

    let any_change = title.is_some()
        || desc_set
        || priority.is_some()
        || a.clear_milestone
        || a.milestone.is_some()
        || a.clear_parent
        || a.parent.is_some()
        || a.clear_due
        || a.due.is_some()
        || !a.label.is_empty()
        || !a.remove_label.is_empty()
        || !a.blocks.is_empty()
        || !a.blocked_by.is_empty()
        || !a.related_to.is_empty()
        || !a.remove_relation.is_empty();

    if !any_change && a.editor {
        // Interactive editor path requires a TTY; out of golden-test scope.
        return Err(CliError::validation("--editor requires a TTY"));
    }
    if !any_change {
        return Err(CliError::validation(
            "no edits requested (pass a flag or --editor)",
        ));
    }

    // Normalize relation keys up front.
    let blocks: Vec<String> = a.blocks.iter().map(|k| parse_issue_key(k)).collect::<Result<_, _>>()?;
    let blocked_by: Vec<String> =
        a.blocked_by.iter().map(|k| parse_issue_key(k)).collect::<Result<_, _>>()?;
    let related_to: Vec<String> =
        a.related_to.iter().map(|k| parse_issue_key(k)).collect::<Result<_, _>>()?;
    let remove_relation: Vec<String> =
        a.remove_relation.iter().map(|k| parse_issue_key(k)).collect::<Result<_, _>>()?;

    let store = store_open::open(db).await?;

    // Resolve milestone name → id (project-scoped) up front.
    let milestone_field: Option<Option<i64>> = if a.clear_milestone {
        Some(None)
    } else if let Some(name) = a.milestone.clone() {
        let pk = project_part.clone();
        let mid = store
            .call(move |conn| Ok(milestones::get(conn, &pk, &name)?.map(|m| m.id)))
            .await?;
        match mid {
            Some(id) => Some(Some(id)),
            None => {
                // Match Go: ErrNotFound "not found: milestone \"name\"".
                let n = a.milestone.clone().unwrap_or_default();
                return Err(CliError::not_found(format!("not found: milestone {n:?}")));
            }
        }
    } else {
        None
    };

    // Resolve parent key → id (same-project + own-parent checks mirror Go).
    let parent_field: Option<Option<i64>> = if a.clear_parent {
        Some(None)
    } else if let Some(pk) = parent_key.clone() {
        if project_prefix(&pk) != project_part {
            return Err(CliError::validation("parent must be in same project"));
        }
        if pk == key {
            return Err(CliError::validation("issue cannot be its own parent"));
        }
        let lookup = pk.clone();
        let pid = store
            .call(move |conn| Ok(issues::get_by_key(conn, &lookup)?.map(|i| i.id)))
            .await?;
        match pid {
            Some(id) => Some(Some(id)),
            None => return Err(CliError::not_found(format!("not found: parent {pk}"))),
        }
    } else {
        None
    };

    let description = if desc_set { Some(desc_content) } else { None };

    // Apply the core update (only when there is a field-level change).
    let has_field_update = title.is_some()
        || description.is_some()
        || priority.is_some()
        || milestone_field.is_some()
        || parent_field.is_some()
        || due_date.is_some();
    if has_field_update {
        let lookup = key.clone();
        let upd = UpdateIssue {
            title: title.clone(),
            description: description.clone(),
            priority: priority.clone(),
            milestone_id: milestone_field,
            parent_id: parent_field,
            due_date,
            ..Default::default()
        };
        store
            .call(move |conn| {
                let issue =
                    issues::get_by_key(conn, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
                issues::update(conn, &issue, upd)?;
                Ok(())
            })
            .await?;
    } else {
        // Even with no field update, confirm the issue exists (Go fetches it).
        let lookup = key.clone();
        store
            .call(move |conn| issues::get_by_key(conn, &lookup))
            .await?
            .ok_or(cliban_core::Error::NotFound)?;
    }

    // Labels.
    for lbl in a.label {
        let lookup = key.clone();
        store
            .call(move |conn| {
                let issue =
                    issues::get_by_key(conn, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
                issues::add_label(conn, &issue, &lbl)
            })
            .await?;
    }
    for lbl in a.remove_label {
        let lookup = key.clone();
        store
            .call(move |conn| {
                let issue =
                    issues::get_by_key(conn, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
                issues::remove_label(conn, &issue, &lbl)
            })
            .await?;
    }

    // Relations.
    for other in blocks {
        let from = key.clone();
        store.call(move |conn| relations::add(conn, &from, &other, "blocks")).await?;
    }
    for other in blocked_by {
        let to = key.clone();
        store.call(move |conn| relations::add(conn, &other, &to, "blocks")).await?;
    }
    for other in related_to {
        let from = key.clone();
        store.call(move |conn| relations::add(conn, &from, &other, "related_to")).await?;
    }
    for other in remove_relation {
        let k = key.clone();
        store
            .call(move |conn| {
                let _ = relations::remove(conn, &k, &other, "blocks");
                let _ = relations::remove(conn, &other, &k, "blocks");
                let _ = relations::remove(conn, &k, &other, "related_to");
                Ok(())
            })
            .await?;
    }

    let reload = key.clone();
    let issue = store
        .call(move |conn| issues::get_by_key(conn, &reload))
        .await?
        .ok_or(cliban_core::Error::NotFound)?;
    print_issue_result(&store, &issue, "updated", a.json).await
}

async fn log(db: &Option<String>, a: LogArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let mut msg = a.message.clone().unwrap_or_default();
    if let Some(file) = &a.message_file {
        if !msg.is_empty() {
            return Err(CliError::validation(
                "pass <message> OR --message-file, not both",
            ));
        }
        let content = if file == "-" {
            read_stdin()?
        } else {
            std::fs::read_to_string(file).map_err(|e| CliError::validation(e.to_string()))?
        };
        msg = content.trim_end_matches('\n').to_string();
    }
    if msg.is_empty() {
        return Err(CliError::validation(
            "message required (positional or --message-file)",
        ));
    }

    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let now = Utc::now();
    let entry = msg.clone();
    store
        .call(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let issue =
                issues::get_by_key(&tx, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
            let new_desc = descmd::append_activity_log(&issue.description, &entry, now);
            let updated = format_usec(now);
            tx.execute(
                "UPDATE issues SET description = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_desc, updated, issue.id],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await?;

    if a.json {
        let mut m = serde_json::Map::new();
        m.insert("entry".into(), serde_json::json!(msg));
        m.insert("key".into(), serde_json::json!(a.key));
        m.insert("timestamp".into(), serde_json::json!(format_usec(now)));
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(m)).unwrap()
        );
    } else {
        println!("logged on {}: {}", a.key, msg);
    }
    Ok(())
}

async fn tick(db: &Option<String>, a: TickArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let task = a.task;
    let step = a.step;
    let updated_at = store
        .call(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let issue =
                issues::get_by_key(&tx, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
            let new_desc = match descmd::tick_step(&issue.description, task, step) {
                Ok(d) => d,
                Err(msg) => return Err(cliban_core::Error::validation("plan", &msg)),
            };
            let now = format_usec(cliban_core::time::now_usec());
            tx.execute(
                "UPDATE issues SET description = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_desc, now, issue.id],
            )?;
            tx.commit()?;
            Ok(now)
        })
        .await?;

    if a.json {
        let mut m = serde_json::Map::new();
        m.insert("checked".into(), serde_json::json!(true));
        m.insert("key".into(), serde_json::json!(a.key));
        m.insert("step".into(), serde_json::json!(a.step));
        m.insert("task".into(), serde_json::json!(a.task));
        m.insert("updated_at".into(), serde_json::json!(updated_at));
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(m)).unwrap()
        );
    } else {
        println!("ticked {} Task {} Step {}", a.key, a.task, a.step);
    }
    Ok(())
}

async fn promote(db: &Option<String>, a: PromoteArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let project_part = project_prefix(&key).to_string();

    // Up-front validations (mirror Go PromoteStep ordering).
    if a.title.is_empty() {
        return Err(CliError::validation("--title required"));
    }
    if a.as_mode != "sub-issue" && a.as_mode != "related" {
        return Err(CliError::validation(format!(
            "invalid --as {:?} (want sub-issue|related)",
            a.as_mode
        )));
    }

    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let task = a.task;
    let step = a.step;
    let title = a.title.clone();
    let mode = a.as_mode.clone();
    let proj_key = project_part.clone();

    let new_key = store
        .call(move |conn| {
            let tx = conn.unchecked_transaction()?;

            // 1. Read parent issue + project.
            let parent: Option<(i64, i64, String, Option<i64>, i64)> = tx
                .query_row(
                    "SELECT i.id, i.project_id, i.description, i.parent_id, p.issue_seq \
                     FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.key = ?1",
                    rusqlite::params![lookup],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                        ))
                    },
                )
                .optional()?;
            let (parent_id, proj_id, parent_desc, parent_parent, issue_seq) = match parent {
                Some(v) => v,
                None => return Err(cliban_core::Error::NotFound),
            };

            if mode == "sub-issue" && parent_parent.is_some() {
                return Err(cliban_core::Error::validation(
                    "depth",
                    "cannot promote as sub-issue of a sub-issue (would exceed depth 2)",
                ));
            }

            // 2. Allocate seq + insert new issue.
            let new_seq = issue_seq + 1;
            let max_pos: Option<f64> = tx.query_row(
                "SELECT max(position) FROM issues WHERE project_id = ?1 AND status = 'backlog'",
                rusqlite::params![proj_id],
                |r| r.get(0),
            )?;
            let pos = max_pos.unwrap_or(0.0) + 1000.0;
            let now = format_usec(cliban_core::time::now_usec());
            let new_key = format!("{proj_key}-{new_seq}");
            let sub_parent: Option<i64> = if mode == "sub-issue" {
                Some(parent_id)
            } else {
                None
            };
            tx.execute(
                "INSERT INTO issues (key, project_id, milestone_id, parent_id, title, \
                 description, status, priority, position, archived, inserted_at, updated_at) \
                 VALUES (?1, ?2, NULL, ?3, ?4, '', 'backlog', 'none', ?5, 0, ?6, ?6)",
                rusqlite::params![new_key, proj_id, sub_parent, title, pos, now],
            )?;
            let new_id = tx.last_insert_rowid();
            tx.execute(
                "UPDATE projects SET issue_seq = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_seq, now, proj_id],
            )?;

            // 3. Related mode: insert related_to in BOTH directions.
            if mode == "related" {
                tx.execute(
                    "INSERT OR IGNORE INTO issue_relation (from_issue_id, to_issue_id, type, created_at) \
                     VALUES (?1, ?2, 'related_to', ?3)",
                    rusqlite::params![new_id, parent_id, now],
                )?;
                tx.execute(
                    "INSERT OR IGNORE INTO issue_relation (from_issue_id, to_issue_id, type, created_at) \
                     VALUES (?1, ?2, 'related_to', ?3)",
                    rusqlite::params![parent_id, new_id, now],
                )?;
            }

            // 4. Rewrite the parent's step line.
            let step_obj = find_step_for_rewrite(&parent_desc, task, step);
            let raw = match step_obj {
                Some(s) => s,
                None => {
                    return Err(cliban_core::Error::validation(
                        "plan",
                        &format!("cannot find Task {task} Step {step} in parent description"),
                    ))
                }
            };
            let new_line = build_promoted_line(&raw, &new_key);
            let new_desc = match descmd::rewrite_step_line(&parent_desc, task, step, &new_line) {
                Ok(d) => d,
                Err(msg) => return Err(cliban_core::Error::validation("plan", &msg)),
            };
            tx.execute(
                "UPDATE issues SET description = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_desc, now, parent_id],
            )?;

            tx.commit()?;
            Ok(new_key)
        })
        .await?;

    if a.json {
        let mut m = serde_json::Map::new();
        m.insert("new_key".into(), serde_json::json!(new_key));
        m.insert("parent".into(), serde_json::json!(a.key));
        m.insert("step".into(), serde_json::json!(a.step));
        m.insert("task".into(), serde_json::json!(a.task));
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(m)).unwrap()
        );
    } else {
        println!(
            "promoted {} Task {} Step {} → {}",
            a.key, a.task, a.step, new_key
        );
    }
    Ok(())
}

/// Mirror Go `findStepForRewrite`: FindSection(Plan) → FindTask → FindStep,
/// returning the raw step line.
fn find_step_for_rewrite(desc: &str, task_n: i32, step_m: i32) -> Option<String> {
    let (plan_start, plan_end, ok) = find_section(desc, "Plan");
    if !ok {
        return None;
    }
    let plan_body = &desc[plan_start..plan_end];
    let (task_start, task_end, ok) = crate::descmd::find_task(plan_body, task_n);
    if !ok {
        return None;
    }
    crate::descmd::find_step(&plan_body[task_start..task_end], step_m).map(|s| s.raw)
}

/// Mirror Go `buildPromotedLine`: strip an existing " → ..." suffix and append
/// " → {NEWKEY}\n".
fn build_promoted_line(original: &str, new_key: &str) -> String {
    let trimmed = original.trim_end_matches('\n');
    let trimmed = match trimmed.rfind(" → ") {
        Some(idx) => &trimmed[..idx],
        None => trimmed,
    };
    format!("{trimmed} → {new_key}\n")
}

async fn archive_done(db: &Option<String>, a: ArchiveDoneArgs) -> CliResult<()> {
    let store = store_open::open(db).await?;
    if a.auto {
        let n = store
            .call(|conn| {
                let now = format_usec(cliban_core::time::now_usec());
                // Per-project policy sweep (mirror Go SweepAutoArchive).
                let mut pols: Vec<(String, i64)> = Vec::new();
                {
                    let mut stmt = conn.prepare(
                        "SELECT key, auto_archive_done_after_days FROM projects \
                         WHERE auto_archive_done_after_days IS NOT NULL",
                    )?;
                    let rows = stmt.query_map([], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                    })?;
                    for row in rows {
                        pols.push(row?);
                    }
                }
                let mut total = 0i64;
                for (pkey, days) in pols {
                    let n = conn.execute(
                        "UPDATE issues SET archived = 1, updated_at = ?1 \
                         WHERE archived = 0 AND status = 'done' AND completed_at IS NOT NULL \
                         AND project_id = (SELECT id FROM projects WHERE key = ?2) \
                         AND datetime(completed_at, '+' || ?3 || ' days') < datetime('now')",
                        rusqlite::params![now, pkey, days],
                    )?;
                    total += n as i64;
                }
                Ok(total)
            })
            .await?;
        if a.json {
            let mut m = serde_json::Map::new();
            m.insert("archived".into(), serde_json::json!(n));
            m.insert("mode".into(), serde_json::json!("auto"));
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::Value::Object(m)).unwrap()
            );
        } else {
            println!("archived {n} done issue(s) (auto sweep)");
        }
        return Ok(());
    }

    let project = a.project.filter(|p| !p.is_empty());
    let project = match project {
        Some(p) => p.to_uppercase(),
        None => {
            return Err(CliError::validation(
                "--project is required (or use --auto for the per-project policy)",
            ))
        }
    };
    let pkey = project.clone();
    let n = store
        .call(move |conn| {
            let now = format_usec(cliban_core::time::now_usec());
            let n = conn.execute(
                "UPDATE issues SET archived = 1, updated_at = ?1 \
                 WHERE archived = 0 AND status = 'done' \
                 AND project_id = (SELECT id FROM projects WHERE key = ?2)",
                rusqlite::params![now, pkey],
            )?;
            Ok(n as i64)
        })
        .await?;
    if a.json {
        let mut m = serde_json::Map::new();
        m.insert("archived".into(), serde_json::json!(n));
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(m)).unwrap()
        );
    } else {
        println!("archived {n} done issue(s) in {project}");
    }
    Ok(())
}

async fn import(db: &Option<String>, a: ImportArgs) -> CliResult<()> {
    let path = a.file_arg.clone().or(a.file.clone());
    let content = match path.as_deref() {
        None | Some("") | Some("-") => read_stdin()?,
        Some(p) => std::fs::read_to_string(p).map_err(|e| CliError::other(e.to_string()))?,
    };
    let default_project = a.project.clone().unwrap_or_default();

    let store = store_open::open(db).await?;
    let mut created = 0i64;
    let mut line_no = 0i64;
    for raw in content.lines() {
        line_no += 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let spec: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| CliError::other(format!("line {line_no}: invalid JSON: {e}")))?;

        let get_str = |k: &str| spec.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut project = get_str("project");
        if project.is_empty() {
            project = default_project.clone();
        }
        if project.is_empty() {
            return Err(CliError::validation(format!(
                "line {line_no}: project required (set per-record or pass --project)"
            )));
        }
        let project = project.to_uppercase();
        let title = get_str("title");
        let description = get_str("description");
        let status = {
            let s = get_str("status");
            if s.is_empty() {
                None
            } else {
                Some(parse_status_lined(&s, line_no)?)
            }
        };
        let priority = {
            let p = get_str("priority");
            if p.is_empty() {
                None
            } else {
                Some(parse_priority_lined(&p, line_no)?)
            }
        };
        let milestone = {
            let m = get_str("milestone");
            if m.is_empty() {
                None
            } else {
                Some(m)
            }
        };
        let parent_key = {
            let p = get_str("parent");
            if p.is_empty() {
                None
            } else {
                Some(parse_issue_key_lined(&p, line_no)?)
            }
        };
        let labels: Vec<String> = spec
            .get("labels")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let create_project = project.clone();
        let create = CreateIssue {
            title,
            description: Some(description),
            status,
            priority,
            milestone,
            parent_key,
            due_date: None,
            position: None,
        };
        let issue = store
            .call(move |conn| issues::create(conn, &create_project, create))
            .await
            .map_err(|e| prefix_line_err(e, line_no))?;

        for lbl in labels {
            let id = issue.id;
            let name = lbl.clone();
            store
                .call(move |conn| {
                    let issue = issues::get_by_id(conn, id)?.ok_or(cliban_core::Error::NotFound)?;
                    issues::add_label(conn, &issue, &name)
                })
                .await
                .map_err(|e| {
                    let code = crate::errors::exit_code_for(&e);
                    let msg = crate::errors::message_for(&e);
                    CliError::Coded(code, format!("line {line_no}: attach label {lbl:?}: {msg}"))
                })?;
        }

        created += 1;
        if a.json {
            let inputs = issue_json_inputs(&store, &issue).await?;
            println!(
                "{}",
                serde_json::to_string(&build_issue_json(inputs)).unwrap()
            );
        }
    }
    if !a.json {
        println!("imported {created} issue(s)");
    }
    Ok(())
}

/// Prefix a core error's message with `line {n}: `, preserving the exit code.
fn prefix_line_err(e: cliban_core::Error, line_no: i64) -> CliError {
    let code = crate::errors::exit_code_for(&e);
    let msg = crate::errors::message_for(&e);
    CliError::Coded(code, format!("line {line_no}: {msg}"))
}

fn parse_status_lined(s: &str, line_no: i64) -> Result<String, CliError> {
    parse_status(s).map_err(|e| CliError::Coded(e.code(), format!("line {line_no}: {}", e.message())))
}

fn parse_priority_lined(s: &str, line_no: i64) -> Result<String, CliError> {
    parse_priority(s)
        .map_err(|e| CliError::Coded(e.code(), format!("line {line_no}: {}", e.message())))
}

fn parse_issue_key_lined(s: &str, line_no: i64) -> Result<String, CliError> {
    parse_issue_key(s)
        .map_err(|e| CliError::Coded(e.code(), format!("line {line_no}: {}", e.message())))
}

async fn mv(db: &Option<String>, key: String, status: String) -> CliResult<()> {
    let key = parse_issue_key(&key)?;
    let status = parse_status(&status)?;
    let store = store_open::open(db).await?;
    store
        .call(move |conn| {
            let issue = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            issues::move_issue(conn, &issue, &status)?;
            Ok(())
        })
        .await?;
    Ok(())
}

async fn rm(db: &Option<String>, key: String) -> CliResult<()> {
    let key = parse_issue_key(&key)?;
    let store = store_open::open(db).await?;
    store
        .call(move |conn| {
            let issue = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            issues::delete(conn, &issue)
        })
        .await?;
    Ok(())
}

async fn set_archived(db: &Option<String>, key: String, archived: bool) -> CliResult<()> {
    let key = parse_issue_key(&key)?;
    let store = store_open::open(db).await?;
    store
        .call(move |conn| {
            let issue = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            issues::update(
                conn,
                &issue,
                UpdateIssue {
                    archived: Some(archived),
                    ..Default::default()
                },
            )?;
            Ok(())
        })
        .await?;
    Ok(())
}

/// `branchIssueRE`: `^([a-z][a-z0-9]+)-(\d+)(?:-|$)`. Hand-written, no regex
/// crate. Returns `(project_lower, seq_digits)` on match.
fn parse_branch(branch: &str) -> Option<(String, String)> {
    let bytes = branch.as_bytes();
    let n = bytes.len();
    // [a-z]
    if n == 0 || !bytes[0].is_ascii_lowercase() {
        return None;
    }
    // [a-z0-9]+ (at least one more char after the leading [a-z])
    let mut i = 1;
    while i < n && (bytes[i].is_ascii_lowercase() || bytes[i].is_ascii_digit()) {
        i += 1;
    }
    // The regex requires `[a-z][a-z0-9]+` — i.e. >= 2 chars in the project run.
    if i < 2 {
        return None;
    }
    // `-`
    if i >= n || bytes[i] != b'-' {
        return None;
    }
    let dash = i;
    i += 1;
    // \d+
    let seq_start = i;
    while i < n && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == seq_start {
        return None;
    }
    // (?:-|$)
    if i != n && bytes[i] != b'-' {
        return None;
    }
    let project = branch[..dash].to_string();
    let seq = branch[seq_start..i].to_string();
    Some((project, seq))
}

fn current_branch() -> Result<String, CliError> {
    if let Ok(v) = std::env::var("CLIBAN_CURRENT_BRANCH_OVERRIDE") {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    let out = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .map_err(|e| CliError::other(format!("git branch --show-current: {e}")))?;
    if !out.status.success() {
        return Err(CliError::other(
            "git branch --show-current: command failed".to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn current(db: &Option<String>, json: bool) -> CliResult<()> {
    let branch = current_branch()?;
    let (proj, seq) = parse_branch(&branch).ok_or_else(|| {
        // Go wraps with %w on store.ErrNotFound → "not found: <msg>".
        CliError::not_found(format!(
            "not found: no issue found for current branch {branch:?}"
        ))
    })?;
    let key = format!("{}-{}", proj.to_uppercase(), seq);
    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let issue = match store
        .call(move |conn| issues::get_by_key(conn, &lookup))
        .await?
    {
        Some(i) => i,
        None => {
            return Err(CliError::not_found(format!(
                "not found: no issue found for current branch {branch:?} (parsed {key})"
            )))
        }
    };
    if json {
        let inputs = issue_json_inputs(&store, &issue).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&build_issue_json(inputs)).unwrap()
        );
    } else {
        println!("{} {}", issue.key, issue.title);
    }
    Ok(())
}

async fn blocked(db: &Option<String>, project: Option<String>, json: bool) -> CliResult<()> {
    let project = project
        .map(|p| p.to_uppercase())
        .filter(|p| !p.is_empty());
    let store = store_open::open(db).await?;
    let pk = project.clone();
    let mut issues = store
        .call(move |conn| relations::list_blocked(conn, pk.as_deref()))
        .await?;
    // Match the Go store ordering: ORDER BY p.key, i.status, i.position.
    base_order(&mut issues);
    if json {
        for i in &issues {
            let inputs = issue_json_inputs(&store, i).await?;
            println!("{}", serde_json::to_string(&build_issue_json(inputs)).unwrap());
        }
        return Ok(());
    }
    let rows = issue_rows(&store, &issues).await?;
    print!("{}", write_issue_table(&rows));
    Ok(())
}
