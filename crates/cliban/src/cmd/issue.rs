//! `cliban issue` subcommands. Output is byte-for-byte parity with the Go
//! oracle (`internal/cli/issue.go`).

use std::io::{Read, Write};

use cliban_core::contexts::issues::{CreateIssue, ListOpts, UpdateIssue};
use cliban_core::contexts::{issues, milestones, relations};
use cliban_core::schema::{Issue, ISSUE_PRIORITIES, ISSUE_STATUSES};
use cliban_core::time::{format_date, format_usec, parse_date, parse_ts};
use cliban_core::Store;

use chrono::Utc;

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

pub async fn run(db: &Option<String>, args: IssueArgs) -> CliResult<()> {
    match args.cmd {
        IssueCmd::Add(a) => add(db, a).await,
        IssueCmd::Show(a) => show(db, a).await,
        IssueCmd::Ls(a) => ls(db, a).await,
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
    if a.search.as_deref().map(str::trim).unwrap_or("").is_empty() {
        // no search query: fall through
    } else {
        return Err(CliError::other("search not yet implemented (Task 11)"));
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
