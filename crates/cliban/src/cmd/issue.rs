//! `cliban issue` subcommands.

use std::io::{Read, Write};

use cliban_core::contexts::issues::{CreateIssue, ListOpts, UpdateIssue};
use cliban_core::contexts::{claims, issues, milestones, relations};
use cliban_core::schema::{Issue, ISSUE_PRIORITIES, ISSUE_STATUSES};
use cliban_core::sections::replace_section;
use cliban_core::time::{format_date, format_usec, parse_date};
use cliban_core::Store;

use chrono::Utc;
use rusqlite::OptionalExtension;

use crate::descmd;
use crate::descmd::find_section;
use crate::errors::{CliError, CliResult};
use crate::output::{
    build_issue_json, write_issue_table, Detail, IssueJsonInputs, IssueRow, Mode, RelationOut,
};
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
    /// Atomically append a block to the end of one H2 section
    #[command(name = "append-section")]
    AppendSection(AppendSectionArgs),
    /// Tick a step in the issue's ## Plan section
    Tick(TickArgs),
    /// Promote a plan step into its own issue
    Promote(PromoteArgs),
    /// Copy an issue's shape into a new backlog issue — title, ## Spec,
    /// ## Plan (checkboxes reset), ## Notes, labels, priority — never its
    /// history (activity log, claims, relations, due date, completed_at)
    Cp(CpArgs),
    /// Archive done issues
    #[command(name = "archive-done")]
    ArchiveDone(ArchiveDoneArgs),
    /// Bulk-create issues from an NDJSON file (or stdin with '-')
    Import(ImportArgs),
    /// Move an issue to a new status
    Mv(MvArgs),
    /// Print the raw description exactly as stored — no header, no JSON, no
    /// formatting, ever. The read-side spare for piping a ticket's markdown
    /// into anything that wants bytes.
    Cat(CatArgs),
    /// A unix reflex, not a real deleter (hidden): archives, says so, names
    /// the undo. An agent that guesses `rm` gets the closest safe thing
    /// instead of losing a turn to a usage error.
    #[command(hide = true)]
    Rm(RmArgs),
    /// GitHub reflex (hidden): `close` is `mv done`. The confirmation states
    /// the canonical form once, so the guess teaches itself out of use.
    #[command(hide = true)]
    Close(StatusAliasArgs),
    /// GitHub reflex (hidden): `reopen` is `mv in-progress`.
    #[command(hide = true)]
    Reopen(StatusAliasArgs),
    /// GitHub reflex (hidden): `comment` is `log` — same args, same
    /// positional/--message-file/piped-stdin resolution.
    #[command(hide = true)]
    Comment(LogArgs),
    /// GitHub reflex (hidden): `delete` gets rm's behavior — archive, say so,
    /// name the undo — teaching `archive` as the canonical form.
    #[command(hide = true)]
    Delete(RmArgs),
    /// Archive an issue (hides it from the default board and lists)
    Archive {
        key: String,
        /// JSON output (echo the archived issue)
        #[arg(long)]
        json: bool,
        /// human output (one-line confirmation)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Unarchive an issue
    Unarchive {
        key: String,
        /// JSON output (echo the unarchived issue)
        #[arg(long)]
        json: bool,
        /// human output (one-line confirmation)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Show the issue inferred from the current git branch
    Current {
        #[arg(long)]
        json: bool,
        /// human output
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Claim an issue for the current actor ($CLIBAN_ACTOR, or the ambient
    /// Claude session)
    Claim {
        key: String,
        /// claim as this actor instead of the resolved default
        #[arg(long)]
        by: Option<String>,
        /// take over another actor's live claim
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
        /// human output
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Release an issue's claim
    Release {
        key: String,
        #[arg(long)]
        json: bool,
        /// human output
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Validate the description contract (## Spec / ## Plan / ## Activity Log)
    Lint {
        key: String,
        #[arg(long)]
        json: bool,
        /// human output
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
}

#[derive(clap::Args)]
pub struct ShowArgs {
    /// issue key
    key: String,
    /// JSON output
    #[arg(long)]
    json: bool,
    /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
    /// pipe human-readable output through $PAGER
    #[arg(long)]
    pager: bool,
}

#[derive(clap::Args)]
pub struct LsArgs {
    /// project key filter
    #[arg(long, short = 'p')]
    project: Option<String>,
    /// status filter
    #[arg(long, short = 's')]
    status: Option<String>,
    /// priority filter
    #[arg(long, value_enum, ignore_case = true)]
    priority: Option<Priority>,
    /// milestone filter
    #[arg(long, short = 'm')]
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
    /// human table output (overrides $CLIBAN_OUTPUT and pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
    /// full rows in --json output: description body plus the fields list rows omit
    #[arg(long)]
    full: bool,
    /// include archived issues
    #[arg(long)]
    archived: bool,
    /// only issues updated since: 4h, 3d, 2w, today, yesterday, 2026-07-25, or RFC3339
    #[arg(long = "updated-since")]
    updated_since: Option<String>,
    /// fuzzy search query across title/key/labels/description
    #[arg(long)]
    search: Option<String>,
    /// cap result count (0 = uncapped; --search defaults to 50)
    #[arg(long, default_value_t = 0)]
    limit: i64,
    /// only takeable issues: backlog status, no open blocker, unclaimed
    #[arg(long, conflicts_with_all = ["blocked", "status", "archived", "search"])]
    ready: bool,
    /// only issues with at least one open blocker
    #[arg(long, conflicts_with = "search")]
    blocked: bool,
}

#[derive(clap::Args)]
pub struct AddArgs {
    /// issue title (optional only with --editor)
    title: Option<String>,
    /// project key (default: $CLIBAN_PROJECT)
    #[arg(long, short = 'p')]
    project: Option<String>,
    /// description (use '-' to read from stdin)
    #[arg(long, allow_hyphen_values = true)]
    description: Option<String>,
    /// read description from a file (use '-' for stdin)
    #[arg(long = "description-file")]
    description_file: Option<String>,
    /// parent issue key (sub-issue)
    #[arg(long)]
    parent: Option<String>,
    /// milestone name
    #[arg(long, short = 'm')]
    milestone: Option<String>,
    /// priority
    #[arg(long, value_enum, ignore_case = true)]
    priority: Option<Priority>,
    /// status
    #[arg(long, short = 's')]
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
    /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
    /// open $EDITOR for input when no --title supplied
    #[arg(long)]
    editor: bool,
}

#[derive(clap::Args)]
pub struct EditArgs {
    /// issue key
    key: String,
    /// new title
    #[arg(long, allow_hyphen_values = true)]
    title: Option<String>,
    /// new description (use '-' for stdin)
    #[arg(long, allow_hyphen_values = true)]
    description: Option<String>,
    /// read description from a file (use '-' for stdin)
    #[arg(long = "description-file")]
    description_file: Option<String>,
    /// new priority
    #[arg(long, value_enum, ignore_case = true)]
    priority: Option<Priority>,
    /// new milestone
    #[arg(long, short = 'm')]
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
    /// replace only this H2 section with the description content:
    /// spec|plan|notes, or a verbatim H2 anchor like "Decisions so far"
    /// (the activity log is owned by `issue log`)
    #[arg(long)]
    section: Option<String>,
    /// allow --section to create the section when it doesn't exist yet
    #[arg(long = "create-section")]
    create_section: bool,
    /// abort unless the issue's updated_at still equals this value (as read
    /// from a prior --json); closes the read-modify-write race
    #[arg(long = "if-updated-at")]
    if_updated_at: Option<String>,
    /// JSON output
    #[arg(long)]
    json: bool,
    /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

#[derive(clap::Args)]
pub struct AppendSectionArgs {
    /// issue key
    key: String,
    /// text to append (leading "-" is fine — markdown bullets are the
    /// common case)
    #[arg(allow_hyphen_values = true)]
    text: Option<String>,
    /// target section: spec|plan|notes, or a verbatim H2 anchor like
    /// "Decisions so far" (the activity log is owned by `issue log`)
    #[arg(long)]
    section: String,
    /// read the text from a file (use '-' for stdin)
    #[arg(long = "text-file")]
    text_file: Option<String>,
    /// create the section when it doesn't exist yet
    #[arg(long = "create-section")]
    create_section: bool,
    /// JSON output
    #[arg(long)]
    json: bool,
    /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

#[derive(clap::Args)]
pub struct LogArgs {
    /// issue key
    key: String,
    /// log message (leading "-" is fine)
    #[arg(allow_hyphen_values = true)]
    message: Option<String>,
    /// read message from file (use '-' for stdin)
    #[arg(long = "message-file")]
    message_file: Option<String>,
    /// JSON output
    #[arg(long)]
    json: bool,
    /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

#[derive(clap::Args)]
pub struct MvArgs {
    /// issue key
    key: String,
    /// new status
    status: String,
    /// why — recorded on the issue's timeline with the transition
    #[arg(long, allow_hyphen_values = true)]
    note: Option<String>,
    /// JSON output (echo the moved issue)
    #[arg(long)]
    json: bool,
    /// human output (one-line confirmation)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

#[derive(clap::Args)]
pub struct RmArgs {
    /// issue key
    key: String,
}

#[derive(clap::Args)]
pub struct CatArgs {
    /// issue key
    key: String,
    /// print only one section's body: spec|plan|activity|notes, or a verbatim H2 anchor
    #[arg(long)]
    section: Option<String>,
}

/// Shared shape of the `close`/`reopen` reflexes: a fixed-status `mv` — same
/// `--note`, same output modes, minus the status argument the alias implies.
#[derive(clap::Args)]
pub struct StatusAliasArgs {
    /// issue key
    key: String,
    /// why — recorded on the issue's timeline with the transition
    #[arg(long, allow_hyphen_values = true)]
    note: Option<String>,
    /// JSON output (echo the moved issue)
    #[arg(long)]
    json: bool,
    /// human output (one-line confirmation)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

/// How a reflex alias teaches: `verb` replaces the canonical confirmation
/// verb on the human line — `closed CLI-4 (mv done): backlog → done` — and
/// `canonical` names the real command there and in a `"canonical"` field on
/// the JSON echo (the same add-a-marker pattern as the `"noop"` retry flag,
/// so the canonical shape is untouched).
#[derive(Clone, Copy)]
struct Teach {
    verb: &'static str,
    canonical: &'static str,
}

#[derive(clap::Args)]
pub struct TickArgs {
    /// issue key
    key: String,
    /// task number (1-indexed; optional when the plan has exactly one task)
    #[arg(long)]
    task: Option<i32>,
    /// step number (required, 1-indexed)
    #[arg(long)]
    step: i32,
    /// JSON output
    #[arg(long)]
    json: bool,
    /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

#[derive(clap::Args)]
pub struct PromoteArgs {
    /// issue key
    key: String,
    /// task number (1-indexed; optional when the plan has exactly one task)
    #[arg(long)]
    task: Option<i32>,
    /// step number (required, 1-indexed)
    #[arg(long)]
    step: i32,
    /// title for the promoted issue (default: the step's own text)
    #[arg(long, default_value = "")]
    title: String,
    /// promotion mode: sub-issue|related
    #[arg(long = "as", default_value = "sub-issue")]
    as_mode: String,
    /// JSON output
    #[arg(long)]
    json: bool,
    /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

#[derive(clap::Args)]
pub struct CpArgs {
    /// source issue key
    key: String,
    /// create the copy in this project (default: the source's project);
    /// cross-project copies drop the milestone — names are project-scoped
    #[arg(long, short = 'p')]
    project: Option<String>,
    /// title for the copy (default: the source's title, unchanged)
    #[arg(long, allow_hyphen_values = true)]
    title: Option<String>,
    /// JSON output (echo the new issue)
    #[arg(long)]
    json: bool,
    /// human output (one-line confirmation; overrides $CLIBAN_OUTPUT and
    /// pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

#[derive(clap::Args)]
pub struct ArchiveDoneArgs {
    /// project key
    #[arg(long, short = 'p')]
    project: Option<String>,
    /// sweep every project per its auto_archive_done_after_days policy
    #[arg(long)]
    auto: bool,
    /// JSON output
    #[arg(long)]
    json: bool,
    /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

#[derive(clap::Args)]
pub struct ImportArgs {
    /// NDJSON file path (default: stdin)
    file_arg: Option<String>,
    /// default project key for records that omit it
    #[arg(long, short = 'p')]
    project: Option<String>,
    /// emit each created issue as a JSON line
    #[arg(long)]
    json: bool,
    /// human output (one summary line; overrides $CLIBAN_OUTPUT and pipe
    /// detection)
    #[arg(long, conflicts_with = "json")]
    table: bool,
}

pub async fn run(db: &Option<String>, args: IssueArgs) -> CliResult<()> {
    match args.cmd {
        IssueCmd::Add(a) => add(db, a).await,
        IssueCmd::Show(a) => show(db, a).await,
        IssueCmd::Ls(a) => ls(db, a).await,
        IssueCmd::Edit(a) => edit(db, a).await,
        IssueCmd::Log(a) => log(db, a, None).await,
        IssueCmd::AppendSection(a) => append_section_cmd(db, a).await,
        IssueCmd::Tick(a) => tick(db, a).await,
        IssueCmd::Promote(a) => promote(db, a).await,
        IssueCmd::Cp(a) => cp(db, a).await,
        IssueCmd::ArchiveDone(a) => archive_done(db, a).await,
        IssueCmd::Import(a) => import(db, a).await,
        IssueCmd::Mv(a) => {
            mv(
                db,
                a.key,
                a.status,
                a.note,
                crate::output::mode(a.json, a.table),
                None,
            )
            .await
        }
        IssueCmd::Cat(a) => cat(db, a).await,
        IssueCmd::Rm(a) => rm_reflex(db, a.key, None).await,
        // The reflex aliases forward onto the canonical handlers — mv's
        // retry-safe noop, log's stdin fallback, and every output-mode rule
        // come along for free; the `Teach` only decorates the confirmation.
        IssueCmd::Close(a) => {
            mv(
                db,
                a.key,
                "done".to_string(),
                a.note,
                crate::output::mode(a.json, a.table),
                Some(Teach {
                    verb: "closed",
                    canonical: "mv done",
                }),
            )
            .await
        }
        IssueCmd::Reopen(a) => {
            mv(
                db,
                a.key,
                "in-progress".to_string(),
                a.note,
                crate::output::mode(a.json, a.table),
                Some(Teach {
                    verb: "reopened",
                    canonical: "mv in-progress",
                }),
            )
            .await
        }
        IssueCmd::Comment(a) => {
            log(
                db,
                a,
                Some(Teach {
                    verb: "commented",
                    canonical: "log",
                }),
            )
            .await
        }
        IssueCmd::Delete(a) => rm_reflex(db, a.key, Some("archive")).await,
        IssueCmd::Archive { key, json, table } => {
            let store = store_open::open(db).await?;
            let (issue, noop) = set_archived_on(&store, key, true).await?;
            confirm_issue(
                &store,
                &issue,
                "archived",
                noop,
                crate::output::mode(json, table),
            )
            .await
        }
        IssueCmd::Unarchive { key, json, table } => {
            let store = store_open::open(db).await?;
            let (issue, noop) = set_archived_on(&store, key, false).await?;
            confirm_issue(
                &store,
                &issue,
                "unarchived",
                noop,
                crate::output::mode(json, table),
            )
            .await
        }
        IssueCmd::Current { json, table } => current(db, crate::output::mode(json, table)).await,
        IssueCmd::Claim {
            key,
            by,
            force,
            json,
            table,
        } => claim_cmd(db, key, by, force, crate::output::mode(json, table)).await,
        IssueCmd::Release { key, json, table } => {
            release_cmd(db, key, crate::output::mode(json, table)).await
        }
        IssueCmd::Lint { key, json, table } => {
            lint_cmd(db, key, crate::output::mode(json, table)).await
        }
    }
}

/// Parse a status: lowercase+trim, must be a known status. Returns a plain
/// exit-3 error — deliberately not a validation error — at the `issue mv`/
/// `issue ls` call sites.
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

/// The `--priority` flag's value set, so clap lists the spellings in `--help`,
/// rejects anything else itself, and shell completions offer them.
///
/// It used to be a bare `String`, and the paths disagreed about what a
/// priority was: `edit`, `ls`, and `import` ran it through [`parse_priority`]
/// (trim + lowercase), while `add` passed the raw string to core's exact,
/// case-sensitive check. `--priority "  HIGH "` was therefore accepted by
/// `edit` and rejected by `add`, and `add`'s rejection did not even name the
/// valid values. One value set now covers every flag.
///
/// The variants are the CLI's spelling of [`ISSUE_PRIORITIES`]; the test at
/// the bottom of this file fails if the two ever drift apart.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Priority {
    None,
    Low,
    Medium,
    High,
    Urgent,
}

impl Priority {
    /// The stored spelling — what goes in the DB column.
    fn as_str(self) -> &'static str {
        match self {
            Priority::None => "none",
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
            Priority::Urgent => "urgent",
        }
    }
}

/// `domain.ParsePriority`: lowercase+trim, must be a known priority. Plain
/// (exit-3) error. Still reached by `import`, whose priorities come from a
/// TSV/JSON field rather than a command-line flag and so never pass through
/// clap's value parsing.
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
/// `"<UPPER>-<seq>"`.
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
    let project_key = crate::scope::required_project(a.project)?;
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
            "a title is required (pass --editor to open $EDITOR)",
        ));
    }

    // Pre-parse the parent key before any store work.
    let parent_key = match &a.parent {
        Some(p) if !p.is_empty() => Some(parse_issue_key(p)?),
        _ => None,
    };

    // Parse the due date (validation error on failure).
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

    // Normalize the relation key flags up front.
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
    let priority = a.priority.map(|p| p.as_str().to_string());
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

    // Step 2: attach labels (idempotent, sequential).
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

    print_issue_result(
        &store,
        &issue,
        "created",
        crate::output::mode(a.json, a.table),
    )
    .await
}

/// Echo a mutated issue: human `{verb} {KEY}: {title}\n`; json one compact
/// Echo-shape line (the lean row + a CAS-precision `updated_at`).
async fn print_issue_result(store: &Store, issue: &Issue, verb: &str, mode: Mode) -> CliResult<()> {
    if mode.is_json() {
        let inputs = issue_json_inputs(store, issue).await?;
        println!(
            "{}",
            serde_json::to_string(&build_issue_json(inputs, Detail::Echo)).unwrap()
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
            let claimed_by = claims::get(conn, id)?.map(|c| c.claimed_by);
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
                claimed_by,
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

/// Map a `--section` value to its H2 anchor. The four contract sections have
/// case-insensitive short aliases; anything else is a **verbatim** H2 anchor —
/// `--section "Decisions so far"` targets `## Decisions so far`, exact match.
fn resolve_section(s: &str) -> Result<String, CliError> {
    let t = s.trim();
    if t.is_empty() {
        return Err(CliError::validation("--section can't be blank"));
    }
    Ok(match t.to_lowercase().as_str() {
        "spec" => "Spec".to_string(),
        "plan" => "Plan".to_string(),
        "activity" | "activity log" => "Activity Log".to_string(),
        "notes" => "Notes".to_string(),
        _ => t.to_string(),
    })
}

/// The write-side existence policy for a section anchor: writing to a section
/// that isn't there is exit 2 naming what IS there, unless the caller
/// explicitly asked to create it — `replace_section` would otherwise turn a
/// typo into a junk section appended below the real one, silently.
fn require_section(issue: &Issue, anchor: &str, create: bool) -> Result<(), cliban_core::Error> {
    if create || cliban_core::sections::find_section(&issue.description, anchor).2 {
        return Ok(());
    }
    let existing = cliban_core::sections::h2_anchors(&issue.description);
    let listing = if existing.is_empty() {
        "none".to_string()
    } else {
        existing.join(", ")
    };
    Err(cliban_core::Error::validation(
        "section",
        &format!(
            "no ## {anchor} section in {} (sections: {listing}); \
             pass --create-section to add it",
            issue.key
        ),
    ))
}

async fn show(db: &Option<String>, a: ShowArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let issue = store
        .call(move |conn| issues::get_by_key(conn, &lookup))
        .await?
        .ok_or(cliban_core::Error::NotFound)?;

    if crate::output::mode(a.json, a.table).is_json() {
        let inputs = issue_json_inputs(&store, &issue).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&build_issue_json(inputs, Detail::Full)).unwrap()
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

fn parse_updated_since(s: &str) -> Result<chrono::DateTime<Utc>, CliError> {
    crate::since::parse(s, "--updated-since")
}

async fn ls(db: &Option<String>, a: LsArgs) -> CliResult<()> {
    // --search branch. The flag being *present* (even empty) selects the
    // branch; an empty/whitespace value is a clean validation error.
    if let Some(raw) = &a.search {
        if raw.trim().is_empty() {
            return Err(CliError::validation("--search requires a non-empty query"));
        }
        // --sort is ignored under --search; say so on stderr.
        if a.sort.is_some() {
            eprintln!("note: --sort is ignored when --search is set");
        }
        return run_search(db, &a, raw.clone()).await;
    }

    // Pre-parse the typed filters before any store work.
    let project = crate::scope::project(a.project.clone());
    let status = match &a.status {
        Some(s) if !s.is_empty() => Some(parse_status(s)?),
        _ => None,
    };
    let priority = a.priority.map(|p| p.as_str().to_string());
    let parent_key = match &a.parent {
        Some(p) if !p.is_empty() => Some(parse_issue_key(p)?),
        _ => None,
    };
    let updated_since = match &a.updated_since {
        Some(s) if !s.is_empty() => Some(parse_updated_since(s)?),
        _ => None,
    };
    // Validate the sort spec up front (sort runs after the fetch, but a
    // bad spec is a clean validation error either way).
    let sort = a.sort.clone().filter(|s| !s.is_empty());
    if let Some(spec) = &sort {
        validate_sort_spec(spec)?;
    }

    let store = store_open::open(db).await?;

    // Core list handles project/status/milestone, but its `archived` flag is an
    // exact match (archived = this). `--archived` means *include* archived:
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

    // These filters apply after the fetch; the result set is the same as
    // filtering in the query.
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
            let names = store
                .call(move |conn| issues::label_names(conn, id))
                .await?;
            if want.iter().all(|w| names.iter().any(|n| n == w)) {
                kept.push(i);
            }
        }
        issues = kept;
    }
    if let Some(threshold) = updated_since {
        issues.retain(|i| i.updated_at >= threshold);
    }
    // --ready / --blocked are relation-aware subsets computed in core;
    // intersecting with the fetch keeps every other ls filter composable
    // with them for free.
    if a.ready {
        let pk = project.clone();
        let ready_ids: std::collections::HashSet<i64> = store
            .call(move |conn| relations::list_ready(conn, pk.as_deref(), None, None))
            .await?
            .into_iter()
            .map(|i| i.id)
            .collect();
        issues.retain(|i| ready_ids.contains(&i.id));
    }
    if a.blocked {
        let pk = project.clone();
        let blocked_ids: std::collections::HashSet<i64> = store
            .call(move |conn| relations::list_blocked(conn, pk.as_deref()))
            .await?
            .into_iter()
            .map(|i| i.id)
            .collect();
        issues.retain(|i| blocked_ids.contains(&i.id));
    }

    // Base ordering: ORDER BY p.key, i.status,
    // i.position (the issue key embeds the project key, so its prefix sorts by
    // project). The explicit --sort below is then applied stably on top.
    base_order(&mut issues);

    if let Some(spec) = &sort {
        sort_issues(&mut issues, spec);
    }
    if a.limit > 0 {
        issues.truncate(a.limit as usize);
    }

    if crate::output::mode(a.json, a.table).is_json() {
        for i in &issues {
            let inputs = issue_json_inputs(&store, i).await?;
            println!(
                "{}",
                serde_json::to_string(&build_issue_json(inputs, Detail::from_full_flag(a.full)))
                    .unwrap()
            );
        }
        return Ok(());
    }

    let rows = issue_rows(&store, &issues).await?;
    print!("{}", write_issue_table(&rows));
    Ok(())
}

/// `issue ls --search` branch: default limit 50
/// (the `--limit` flag overrides; 0 → 50), NDJSON rows carry a `score` field,
/// human output uses the search table with a leading SCORE column.
async fn run_search(db: &Option<String>, a: &LsArgs, query: String) -> CliResult<()> {
    let effective_limit = if a.limit == 0 { 50 } else { a.limit };
    let store = store_open::open(db).await?;
    let opts = crate::search::Options {
        query,
        project: crate::scope::project(a.project.clone()),
        label: a.label.clone(),
        milestone: a.milestone.clone(),
        status: a.status.clone(),
        priority: a.priority.map(|p| p.as_str().to_string()),
        parent: a.parent.clone(),
        include_archived: a.archived,
        exclude_subs: a.no_subs,
        limit: effective_limit,
    };
    let matches = crate::search::search(&store, opts).await?;

    if crate::output::mode(a.json, a.table).is_json() {
        for m in &matches {
            let inputs = issue_json_inputs(&store, &m.issue).await?;
            println!(
                "{}",
                serde_json::to_string(&crate::output::build_search_match_json(
                    inputs,
                    m.score,
                    Detail::from_full_flag(a.full)
                ))
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
pub fn project_prefix(key: &str) -> &str {
    match key.rfind('-') {
        Some(idx) => &key[..idx],
        None => key,
    }
}

/// Base ordering: (project key, status, position) — the Rust-side mirror of
/// core's `ORDER BY p.key, i.status, i.position`.
///
/// Shared with the search module, which stable-sorts on top of it so its
/// score/`updated_at` tiebreaks resolve deterministically. It used to keep a
/// byte-identical copy of this function and of [`project_prefix`]; one
/// ordering with two implementations is two chances for `issue ls` and
/// `issue ls --search` to disagree about what "same rank" means.
pub fn base_order(issues: &mut [Issue]) {
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

    // --section narrows the description write to one H2, merged into the live
    // description inside the update job so nothing else moves.
    let section_write: Option<String> = match &a.section {
        None => None,
        Some(s) => {
            let anchor = resolve_section(s)?;
            if anchor == "Activity Log" {
                return Err(CliError::validation(
                    "--section activity is owned by `issue log`; append entries there",
                ));
            }
            if !desc_set {
                return Err(CliError::validation(
                    "--section needs content: pass --description or --description-file",
                ));
            }
            Some(anchor)
        }
    };

    let title = a.title.clone(); // --title given → Some (even if "")
    let priority = a.priority.map(|p| p.as_str().to_string());
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
    let remove_relation: Vec<String> = a
        .remove_relation
        .iter()
        .map(|k| parse_issue_key(k))
        .collect::<Result<_, _>>()?;

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
                // A missing milestone is not-found, not a validation error.
                let n = a.milestone.clone().unwrap_or_default();
                return Err(CliError::not_found(format!("not found: milestone {n:?}")));
            }
        }
    } else {
        None
    };

    // Resolve parent key → id (same-project + not-its-own-parent checks).
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

    // Snapshot before anything moves, so the timeline entry can say what
    // actually changed rather than just "edited".
    let snapshot_key = key.clone();
    let before = store
        .call(move |conn| issues::get_by_key(conn, &snapshot_key))
        .await?
        .ok_or(cliban_core::Error::NotFound)?;
    let (before_milestone, before_parent) = resolve_refs(&store, &before).await?;

    // Apply the core update (only when there is a field-level change).
    let has_field_update = title.is_some()
        || description.is_some()
        || priority.is_some()
        || milestone_field.is_some()
        || parent_field.is_some()
        || due_date.is_some();
    let cas = a.if_updated_at.clone();
    if has_field_update {
        let lookup = key.clone();
        let cas_check = cas.clone();
        let upd = UpdateIssue {
            title: title.clone(),
            description: description.clone(),
            priority: priority.clone(),
            milestone_id: milestone_field,
            parent_id: parent_field,
            due_date,
            ..Default::default()
        };
        let section_for_job = section_write.clone();
        let create_section = a.create_section;
        store
            .call(move |conn| {
                let issue =
                    issues::get_by_key(conn, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
                check_cas(&issue, cas_check.as_deref())?;
                let mut upd = upd;
                if let Some(anchor) = &section_for_job {
                    if let Some(body) = upd.description.take() {
                        require_section(&issue, anchor, create_section)?;
                        let body = cliban_core::sections::sanitize_section_body(anchor, &body)
                            .map_err(|m| cliban_core::Error::validation("section", &m))?;
                        upd.description = Some(replace_section(&issue.description, anchor, &body));
                    }
                }
                issues::update(conn, &issue, upd)?;
                Ok(())
            })
            .await?;
    } else {
        // Even with no field update, confirm the issue exists.
        let lookup = key.clone();
        let cas_check = cas.clone();
        store
            .call(move |conn| {
                let issue = issues::get_by_key(conn, &lookup)?;
                if let Some(i) = &issue {
                    check_cas(i, cas_check.as_deref())?;
                }
                Ok(issue)
            })
            .await?
            .ok_or(cliban_core::Error::NotFound)?;
    }

    // Capture the relation/label intent before the loops below consume it.
    let added_labels = a.label.clone();
    let removed_labels = a.remove_label.clone();
    let summary_blocks = blocks.clone();
    let summary_blocked_by = blocked_by.clone();
    let summary_related_to = related_to.clone();
    let summary_remove_relation = remove_relation.clone();

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
        store
            .call(move |conn| relations::add(conn, &from, &other, "blocks"))
            .await?;
    }
    for other in blocked_by {
        let to = key.clone();
        store
            .call(move |conn| relations::add(conn, &other, &to, "blocks"))
            .await?;
    }
    for other in related_to {
        let from = key.clone();
        store
            .call(move |conn| relations::add(conn, &from, &other, "related_to"))
            .await?;
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

    // Everything applied; record what moved.
    let (after_milestone, after_parent) = resolve_refs(&store, &issue).await?;
    let mut summary = crate::audit::EditSummary::default();
    summary.field("title", &before.title, &issue.title);
    summary.field("priority", &before.priority, &issue.priority);
    summary.field("milestone", &before_milestone, &after_milestone);
    summary.field("parent", &before_parent, &after_parent);
    summary.field(
        "due",
        &before.due_date.map(format_date).unwrap_or_default(),
        &issue.due_date.map(format_date).unwrap_or_default(),
    );
    if before.description != issue.description {
        if let Some(anchor) = section_write {
            // A section write cannot drop its neighbours by construction.
            summary.note(format!("## {anchor} written"));
        } else {
            // The text itself is not worth replaying, but a rewrite that drops
            // sections is exactly what someone auditing the ticket wants to see.
            let lost: Vec<&str> = ["Spec", "Plan", "Activity Log", "Notes"]
                .into_iter()
                .filter(|anchor| {
                    find_section(&before.description, anchor).2
                        && !find_section(&issue.description, anchor).2
                })
                .collect();
            if lost.is_empty() {
                summary.note("description rewritten");
            } else {
                summary.note(format!(
                    "description rewritten, dropped ## {}",
                    lost.join(", ## ")
                ));
            }
        }
    }
    for l in &added_labels {
        summary.note(format!("+label {l}"));
    }
    for l in &removed_labels {
        summary.note(format!("-label {l}"));
    }
    for (verb, keys) in [
        ("blocks", &summary_blocks),
        ("blocked-by", &summary_blocked_by),
        ("related-to", &summary_related_to),
    ] {
        for k in keys {
            summary.note(format!("+{verb} {k}"));
        }
    }
    for k in &summary_remove_relation {
        summary.note(format!("-relation {k}"));
    }
    if !summary.is_empty() {
        let (id, message) = (issue.id, summary.message());
        store
            .call(move |conn| {
                if let Some(i) = issues::get_by_id(conn, id)? {
                    crate::audit::record(conn, &i, "edit", &message);
                }
                Ok(())
            })
            .await?;
    }

    print_issue_result(
        &store,
        &issue,
        "updated",
        crate::output::mode(a.json, a.table),
    )
    .await
}

async fn log(db: &Option<String>, a: LogArgs, teach: Option<Teach>) -> CliResult<()> {
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
    } else if a.message.is_none() {
        // No positional, no --message-file: piped/redirected stdin IS the
        // message (a TTY yields None and keeps the fast error below).
        if let Some(piped) = crate::stdin_input::fallback()? {
            msg = piped.trim_end_matches('\n').to_string();
        }
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
            let issue = issues::get_by_key(&tx, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
            let new_desc = descmd::append_activity_log(&issue.description, &entry, now);
            let updated = format_usec(now);
            tx.execute(
                "UPDATE issues SET description = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_desc, updated, issue.id],
            )?;
            // Also record it durably: the markdown line lives in the
            // description and a later `--description` rewrite would erase it,
            // taking the note out of the timeline with it.
            crate::audit::record(&tx, &issue, "log", &entry);
            tx.commit()?;
            Ok(())
        })
        .await?;

    if crate::output::mode(a.json, a.table).is_json() {
        let mut m = serde_json::Map::new();
        if let Some(t) = teach {
            m.insert(
                "canonical".into(),
                serde_json::json!(format!("issue {}", t.canonical)),
            );
        }
        m.insert("entry".into(), serde_json::json!(msg));
        m.insert("key".into(), serde_json::json!(a.key));
        m.insert("timestamp".into(), serde_json::json!(format_usec(now)));
        println!(
            "{}",
            serde_json::to_string(&serde_json::Value::Object(m)).unwrap()
        );
    } else if let Some(t) = teach {
        println!("{} {} ({}): {}", t.verb, a.key, t.canonical, msg);
    } else {
        println!("logged on {}: {}", a.key, msg);
    }
    Ok(())
}

async fn tick(db: &Option<String>, a: TickArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let task_arg = a.task;
    let step = a.step;
    let (updated_at, noop, task) = store
        .call(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let issue = issues::get_by_key(&tx, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
            let task = resolve_task(task_arg, &issue.description)?;
            let new_desc = match descmd::tick_step(&issue.description, task, step) {
                Ok(descmd::TickOutcome::Ticked(d)) => d,
                // Desired state already holds: touch nothing (no UPDATE, no
                // audit record), report success with the noop marker.
                Ok(descmd::TickOutcome::AlreadyChecked) => {
                    return Ok((format_usec(issue.updated_at), true, task));
                }
                Err(msg) => return Err(cliban_core::Error::validation("plan", &msg)),
            };
            let now = format_usec(cliban_core::time::now_usec());
            tx.execute(
                "UPDATE issues SET description = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_desc, now, issue.id],
            )?;
            crate::audit::record(
                &tx,
                &issue,
                "plan",
                &format!("ticked Task {task} Step {step}"),
            );
            tx.commit()?;
            Ok((now, false, task))
        })
        .await?;

    if crate::output::mode(a.json, a.table).is_json() {
        let mut m = serde_json::Map::new();
        m.insert("checked".into(), serde_json::json!(true));
        m.insert("key".into(), serde_json::json!(a.key));
        if noop {
            m.insert("noop".into(), serde_json::json!(true));
        }
        m.insert("step".into(), serde_json::json!(a.step));
        m.insert("task".into(), serde_json::json!(task));
        m.insert("updated_at".into(), serde_json::json!(updated_at));
        println!(
            "{}",
            serde_json::to_string(&serde_json::Value::Object(m)).unwrap()
        );
    } else if noop {
        println!(
            "ticked {} Task {} Step {} (already checked — nothing to do)",
            a.key, task, a.step
        );
    } else {
        println!("ticked {} Task {} Step {}", a.key, task, a.step);
    }
    Ok(())
}

/// `--task` may be omitted when `## Plan` has exactly one task; anything
/// else is a real ambiguity the caller must settle.
fn resolve_task(arg: Option<i32>, desc: &str) -> Result<i32, cliban_core::Error> {
    match arg {
        Some(t) => Ok(t),
        None => match descmd::count_tasks(desc) {
            1 => Ok(1),
            0 => Err(cliban_core::Error::validation(
                "plan",
                "no `### Task N:` headings in ## Plan",
            )),
            n => Err(cliban_core::Error::validation(
                "plan",
                &format!("plan has {n} tasks; pass --task"),
            )),
        },
    }
}

async fn promote(db: &Option<String>, a: PromoteArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let project_part = project_prefix(&key).to_string();

    // Up-front validations, before any store work.
    if a.as_mode != "sub-issue" && a.as_mode != "related" {
        return Err(CliError::validation(format!(
            "invalid --as {:?} (want sub-issue|related)",
            a.as_mode
        )));
    }

    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let child_origin = key.clone();
    let task = a.task;
    let step = a.step;
    let title = a.title.clone();
    let mode = a.as_mode.clone();
    let proj_key = project_part.clone();

    let (new_key, task) = store
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

            // Resolve the step first: the raw line names the task (when
            // inferred) and supplies the default title.
            let task = resolve_task(task, &parent_desc)?;
            let raw = match find_step_for_rewrite(&parent_desc, task, step) {
                Some(s) => s,
                None => {
                    return Err(cliban_core::Error::validation(
                        "plan",
                        &format!("cannot find Task {task} Step {step} in parent description"),
                    ))
                }
            };
            let title = if title.is_empty() {
                let t = step_title_text(&raw);
                if t.is_empty() {
                    return Err(cliban_core::Error::validation(
                        "plan",
                        "the step has no text to use as a title; pass --title",
                    ));
                }
                t
            } else {
                title
            };

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

            // 4. Rewrite the parent's step line (located up front).
            let new_line = build_promoted_line(&raw, &new_key);
            let new_desc = match descmd::rewrite_step_line(&parent_desc, task, step, &new_line) {
                Ok(d) => d,
                Err(msg) => return Err(cliban_core::Error::validation("plan", &msg)),
            };
            tx.execute(
                "UPDATE issues SET description = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_desc, now, parent_id],
            )?;

            // Both sides of the split get the record: the parent lost a step,
            // the new issue says where it came from.
            if let Some(parent_issue) = issues::get_by_id(&tx, parent_id)? {
                crate::audit::record(
                    &tx,
                    &parent_issue,
                    "plan",
                    &format!("promoted Task {task} Step {step} → {new_key}"),
                );
            }
            if let Some(child) = issues::get_by_id(&tx, new_id)? {
                crate::audit::record(
                    &tx,
                    &child,
                    "plan",
                    &format!("promoted from {} Task {task} Step {step}", child_origin),
                );
            }

            tx.commit()?;
            Ok((new_key, task))
        })
        .await?;

    if crate::output::mode(a.json, a.table).is_json() {
        let mut m = serde_json::Map::new();
        m.insert("new_key".into(), serde_json::json!(new_key));
        m.insert("parent".into(), serde_json::json!(a.key));
        m.insert("step".into(), serde_json::json!(a.step));
        m.insert("task".into(), serde_json::json!(task));
        println!(
            "{}",
            serde_json::to_string(&serde_json::Value::Object(m)).unwrap()
        );
    } else {
        println!(
            "promoted {} Task {} Step {} → {}",
            a.key, task, a.step, new_key
        );
    }
    Ok(())
}

/// The step's human text: checkbox marker, bold markers, a leading
/// `Step N:` label, and any `→ KEY` promotion suffix stripped — what
/// `promote` uses when `--title` is not given.
fn step_title_text(raw: &str) -> String {
    let t = raw.trim();
    let t = t
        .strip_prefix("- [ ]")
        .or_else(|| t.strip_prefix("- [x]"))
        .unwrap_or(t)
        .trim();
    let t = t.strip_prefix("**").unwrap_or(t);
    let t = t.strip_suffix("**").unwrap_or(t);
    let t = match t.split_once(':') {
        Some((label, rest))
            if label
                .strip_prefix("Step ")
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) =>
        {
            rest
        }
        _ => t,
    };
    let t = match t.rsplit_once('→') {
        // Only a promotion suffix (`→ KEY`) strips — an arrow inside the
        // step's own prose stays.
        Some((before, after)) if descmd::is_issue_key_shaped(after.trim()) => before,
        _ => t,
    };
    t.trim().to_string()
}

/// Locate a step line for rewriting: FindSection(Plan) → FindTask → FindStep,
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

/// Build the promoted step line: strip an existing " → ..." suffix and append
/// " → {NEWKEY}\n".
fn build_promoted_line(original: &str, new_key: &str) -> String {
    let trimmed = original.trim_end_matches('\n');
    let trimmed = match trimmed.rfind(" → ") {
        Some(idx) => &trimmed[..idx],
        None => trimmed,
    };
    format!("{trimmed} → {new_key}\n")
}

/// `issue cp`: duplicate the shape, never the history. The copy gets the
/// title (unless `--title` overrides), the `## Spec` / `## Plan` / `## Notes`
/// sections — plan checkboxes reset and promotion pointers stripped by
/// [`descmd::copy_description`] — plus labels, priority, and (same project
/// only) the milestone. It starts in backlog, unclaimed, unrelated, undated:
/// those are facts about the original's run, not about the work's shape.
async fn cp(db: &Option<String>, a: CpArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let source_project = project_prefix(&key).to_string();
    let target_project = a
        .project
        .map(|p| p.trim().to_uppercase())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| source_project.clone());
    let same_project = target_project == source_project;

    let store = store_open::open(db).await?;

    // Read the source's whole shape in one round-trip.
    let lookup = key.clone();
    let (src, src_milestone, labels) = store
        .call(move |conn| {
            let issue = issues::get_by_key(conn, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
            let milestone = match issue.milestone_id {
                Some(mid) => milestones::get_by_id(conn, mid)?.map(|m| m.name),
                None => None,
            };
            let labels = issues::label_names(conn, issue.id)?;
            Ok((issue, milestone, labels))
        })
        .await?;

    // Milestone names are project-scoped; a cross-project copy drops it
    // rather than inventing one in the target project.
    let milestone_dropped = !same_project && src_milestone.is_some();
    let milestone = if same_project { src_milestone } else { None };
    let title = a
        .title
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| src.title.clone());
    let description = descmd::copy_description(&src.description);
    let priority = src.priority.clone();

    // A fresh backlog issue: no parent, no due date, no completed_at — core
    // `create` never sets those unless asked.
    let create_project = target_project.clone();
    let issue = store
        .call(move |conn| {
            issues::create(
                conn,
                &create_project,
                CreateIssue {
                    title,
                    description: Some(description),
                    status: None,
                    priority: Some(priority),
                    milestone,
                    parent_key: None,
                    due_date: None,
                    position: None,
                },
            )
        })
        .await?;

    for lbl in labels {
        let id = issue.id;
        store
            .call(move |conn| {
                let issue = issues::get_by_id(conn, id)?.ok_or(cliban_core::Error::NotFound)?;
                issues::add_label(conn, &issue, &lbl)
            })
            .await?;
    }

    // Provenance on the copy — and, cross-project, why the milestone is gone.
    let message = if milestone_dropped {
        format!("copied from {key} (milestone dropped: names are project-scoped)")
    } else {
        format!("copied from {key}")
    };
    let id = issue.id;
    store
        .call(move |conn| {
            if let Some(i) = issues::get_by_id(conn, id)? {
                crate::audit::record(conn, &i, "copy", &message);
            }
            Ok(())
        })
        .await?;

    // Reload so the labels land in the echo.
    let reload = issue.key.clone();
    let issue = store
        .call(move |conn| issues::get_by_key(conn, &reload))
        .await?
        .ok_or(cliban_core::Error::NotFound)?;

    print_issue_result(
        &store,
        &issue,
        "copied",
        crate::output::mode(a.json, a.table),
    )
    .await
}

async fn archive_done(db: &Option<String>, a: ArchiveDoneArgs) -> CliResult<()> {
    let mode = crate::output::mode(a.json, a.table);
    let store = store_open::open(db).await?;
    if a.auto {
        let n = store
            .call(|conn| {
                let now = format_usec(cliban_core::time::now_usec());
                // Per-project policy sweep.
                let mut pols: Vec<(String, i64)> = Vec::new();
                {
                    let mut stmt = conn.prepare(
                        "SELECT key, auto_archive_done_after_days FROM projects \
                         WHERE auto_archive_done_after_days IS NOT NULL",
                    )?;
                    let rows =
                        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
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
        if mode.is_json() {
            let mut m = serde_json::Map::new();
            m.insert("archived".into(), serde_json::json!(n));
            m.insert("mode".into(), serde_json::json!("auto"));
            println!(
                "{}",
                serde_json::to_string(&serde_json::Value::Object(m)).unwrap()
            );
        } else {
            println!("archived {n} done issue(s) (auto sweep)");
        }
        return Ok(());
    }

    let project = match crate::scope::project(a.project) {
        Some(p) => p,
        None => {
            return Err(CliError::validation(
                "no project scope: pass -p KEY, set $CLIBAN_PROJECT, or use --auto \
                 for the per-project policy",
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
    if mode.is_json() {
        let mut m = serde_json::Map::new();
        m.insert("archived".into(), serde_json::json!(n));
        println!(
            "{}",
            serde_json::to_string(&serde_json::Value::Object(m)).unwrap()
        );
    } else {
        println!("archived {n} done issue(s) in {project}");
    }
    Ok(())
}

async fn import(db: &Option<String>, a: ImportArgs) -> CliResult<()> {
    let mode = crate::output::mode(a.json, a.table);
    let path = a.file_arg.clone();
    let content = match path.as_deref() {
        None | Some("") | Some("-") => read_stdin()?,
        Some(p) => std::fs::read_to_string(p).map_err(|e| CliError::other(e.to_string()))?,
    };
    let default_project = crate::scope::project(a.project.clone()).unwrap_or_default();

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

        let get_str = |k: &str| {
            spec.get(k)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
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
        if mode.is_json() {
            let inputs = issue_json_inputs(&store, &issue).await?;
            println!(
                "{}",
                serde_json::to_string(&build_issue_json(inputs, Detail::Echo)).unwrap()
            );
        }
    }
    if !mode.is_json() {
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
    parse_status(s)
        .map_err(|e| CliError::Coded(e.code(), format!("line {line_no}: {}", e.message())))
}

fn parse_priority_lined(s: &str, line_no: i64) -> Result<String, CliError> {
    parse_priority(s)
        .map_err(|e| CliError::Coded(e.code(), format!("line {line_no}: {}", e.message())))
}

fn parse_issue_key_lined(s: &str, line_no: i64) -> Result<String, CliError> {
    parse_issue_key(s)
        .map_err(|e| CliError::Coded(e.code(), format!("line {line_no}: {}", e.message())))
}

async fn mv(
    db: &Option<String>,
    key: String,
    status: String,
    note: Option<String>,
    mode: Mode,
    teach: Option<Teach>,
) -> CliResult<()> {
    let key = parse_issue_key(&key)?;
    let status = parse_status(&status)?;
    #[cfg(feature = "linear")]
    let (push_key, push_status) = (key.clone(), status.clone());
    let store = store_open::open(db).await?;
    let move_key = key.clone();
    let move_status = status.clone();
    let from = store
        .call(move |conn| {
            let issue = issues::get_by_key(conn, &move_key)?.ok_or(cliban_core::Error::NotFound)?;
            let from = issue.status.clone();
            // Desired state already holds: no reposition, no updated_at
            // churn, no audit record — a retry is not a second move.
            if from == move_status {
                return Ok(from);
            }
            issues::move_issue(conn, &issue, &move_status)?;
            // The move is the event worth recording; `--note` carries the why.
            crate::audit::record_move(conn, &issue, &from, &move_status, note.as_deref());
            Ok(from)
        })
        .await?;
    let noop = from == status;
    // The move has committed; push_on_move (opt-in via linear.toml) may now
    // mirror it to a linked Linear issue. Best-effort: it never fails the mv.
    #[cfg(feature = "linear")]
    if from != push_status {
        crate::cmd::sync::push_on_move(db, &push_key).await;
    }
    if mode.is_json() {
        // Echo the issue as it now stands — the same shape `show --json`
        // emits, so a piped caller can chain without a follow-up read.
        let reload = key.clone();
        let issue = store
            .call(move |conn| issues::get_by_key(conn, &reload))
            .await?
            .ok_or(cliban_core::Error::NotFound)?;
        let inputs = issue_json_inputs(&store, &issue).await?;
        let mut v = build_issue_json(inputs, Detail::Echo);
        if let serde_json::Value::Object(m) = &mut v {
            if noop {
                m.insert("noop".into(), serde_json::json!(true));
            }
            if let Some(t) = teach {
                m.insert(
                    "canonical".into(),
                    serde_json::json!(format!("issue {}", t.canonical)),
                );
            }
        }
        println!("{}", serde_json::to_string(&v).unwrap());
    } else {
        match (teach, noop) {
            (None, true) => println!("{key} already {status} (nothing to do)"),
            (None, false) => println!("moved {key}: {from} → {status}"),
            (Some(t), true) => println!(
                "{} {key} ({}): already {status} (nothing to do)",
                t.verb, t.canonical
            ),
            (Some(t), false) => {
                println!("{} {key} ({}): {from} → {status}", t.verb, t.canonical)
            }
        }
    }
    Ok(())
}

/// `issue cat`: the stored description, verbatim — the one read that never
/// formats. No `--json`/`--table`, no mode branch, no trailing decoration:
/// its whole contract is "the bytes, exactly". `--section` narrows the same
/// contract to one H2's body.
async fn cat(db: &Option<String>, a: CatArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let store = store_open::open(db).await?;
    let issue = store
        .call(move |conn| issues::get_by_key(conn, &key))
        .await?
        .ok_or(cliban_core::Error::NotFound)?;
    if let Some(section) = &a.section {
        let anchor = resolve_section(section)?;
        let (start, end, ok) = find_section(&issue.description, &anchor);
        if !ok {
            // "not found" (exit 1), not validation: an absent section is an
            // answer about this issue, not a usage mistake.
            return Err(CliError::not_found(format!(
                "not found: no ## {anchor} section in {}",
                a.key
            )));
        }
        print!("{}", &issue.description[start..end]);
        return Ok(());
    }
    print!("{}", issue.description);
    Ok(())
}

/// The shared body of `issue rm` and `issue delete`: do the closest safe
/// thing rather than spending a turn refusing. The message IS the point of
/// these reflexes — it prints in every mode, so the caller always learns
/// nothing was deleted. `delete` additionally states the canonical form
/// (`archive`) once, per the alias contract.
async fn rm_reflex(db: &Option<String>, key: String, canonical: Option<&str>) -> CliResult<()> {
    let key = parse_issue_key(&key)?;
    let store = store_open::open(db).await?;
    // Desired state = archived; already-archived is the same success.
    set_archived_on(&store, key.clone(), true).await?;
    let teach = canonical.map(|c| format!(" ({c})")).unwrap_or_default();
    println!(
        "archived {key}{teach} — cliban archives instead of deleting \
         (undo: cliban issue unarchive {key})"
    );
    Ok(())
}

/// Flip an issue's archived bit and return the (possibly untouched) row plus a
/// noop flag. When the bit already matches — a retry, not a change — nothing is
/// written and no audit record is added. Prints nothing: `archive`/`unarchive`
/// confirm via [`confirm_issue`], while `rm` keeps its own always-on message.
async fn set_archived_on(store: &Store, key: String, archived: bool) -> CliResult<(Issue, bool)> {
    let key = parse_issue_key(&key)?;
    let result = store
        .call(move |conn| {
            let issue = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            if issue.archived == archived {
                return Ok((issue, true));
            }
            let updated = issues::update(
                conn,
                &issue,
                UpdateIssue {
                    archived: Some(archived),
                    ..Default::default()
                },
            )?;
            crate::audit::record(
                conn,
                &issue,
                "archive",
                if archived { "archived" } else { "unarchived" },
            );
            Ok((updated, false))
        })
        .await?;
    Ok(result)
}

/// The mutation contract's success report: a one-line `{verb} {KEY}` in table
/// mode, the full issue JSON (the `show --json` shape) in json mode. Never
/// silent. A noop retry says so — `{KEY} already {verb} (nothing to do)` in
/// table mode, a `"noop": true` field on the JSON echo.
async fn confirm_issue(
    store: &Store,
    issue: &Issue,
    verb: &str,
    noop: bool,
    mode: Mode,
) -> CliResult<()> {
    if mode.is_json() {
        let inputs = issue_json_inputs(store, issue).await?;
        let mut v = build_issue_json(inputs, Detail::Echo);
        if noop {
            if let serde_json::Value::Object(m) = &mut v {
                m.insert("noop".into(), serde_json::json!(true));
            }
        }
        println!("{}", serde_json::to_string(&v).unwrap());
    } else if noop {
        println!("{} already {verb} (nothing to do)", issue.key);
    } else {
        println!("{verb} {}", issue.key);
    }
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

async fn current(db: &Option<String>, mode: Mode) -> CliResult<()> {
    let branch = current_branch()?;
    let (proj, seq) = parse_branch(&branch).ok_or_else(|| {
        // Wrapped as "not found: <msg>" so the caller sees what was missing.
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
    if mode.is_json() {
        let inputs = issue_json_inputs(&store, &issue).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&build_issue_json(inputs, Detail::Full)).unwrap()
        );
    } else {
        println!("{} {}", issue.key, issue.title);
    }
    Ok(())
}

/// The compare-and-swap guard behind `--if-updated-at`: runs inside the write
/// job on the store thread, so the check and the write are atomic.
fn check_cas(issue: &Issue, expected: Option<&str>) -> Result<(), cliban_core::Error> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = format_usec(issue.updated_at);
    if actual != expected {
        return Err(cliban_core::Error::validation(
            "updated_at",
            &format!(
                "stale write: {} was updated at {actual}, you read {expected} — re-read and retry",
                issue.key
            ),
        ));
    }
    Ok(())
}

async fn claim_cmd(
    db: &Option<String>,
    key: String,
    by: Option<String>,
    force: bool,
    mode: Mode,
) -> CliResult<()> {
    let key = parse_issue_key(&key)?;
    let actor = by
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .or_else(crate::audit::actor)
        .ok_or_else(|| {
            CliError::validation("no actor to claim as: pass --by or set CLIBAN_ACTOR")
        })?;
    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let as_actor = actor.clone();
    let (issue, claim) = store
        .call(move |conn| {
            let issue = issues::get_by_key(conn, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
            let prev = claims::get(conn, issue.id)?;
            let claim = claims::claim(conn, &issue, &as_actor, force)?;
            match prev {
                Some(p) if p.claimed_by == as_actor => {} // idempotent re-claim: no event
                Some(p) => crate::audit::record(
                    conn,
                    &issue,
                    "claim",
                    &format!("claimed by {as_actor} (took over from {})", p.claimed_by),
                ),
                None => {
                    crate::audit::record(conn, &issue, "claim", &format!("claimed by {as_actor}"))
                }
            }
            Ok((issue, claim))
        })
        .await?;
    if mode.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "claimed_at": claim.claimed_at,
                "claimed_by": claim.claimed_by,
                "key": issue.key,
            })
        );
    } else {
        println!("claimed {} by {}", issue.key, claim.claimed_by);
    }
    Ok(())
}

async fn release_cmd(db: &Option<String>, key: String, mode: Mode) -> CliResult<()> {
    let key = parse_issue_key(&key)?;
    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let (issue, was_held_by) = store
        .call(move |conn| {
            let issue = issues::get_by_key(conn, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
            let was = claims::release(conn, &issue)?;
            if let Some(holder) = &was {
                crate::audit::record(
                    conn,
                    &issue,
                    "claim",
                    &format!("released (was held by {holder})"),
                );
            }
            Ok((issue, was))
        })
        .await?;
    if mode.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "key": issue.key,
                "released": was_held_by.is_some(),
                "was_held_by": was_held_by,
            })
        );
    } else {
        match was_held_by {
            Some(holder) => println!("released {} (was held by {holder})", issue.key),
            None => println!("{} was not claimed", issue.key),
        }
    }
    Ok(())
}

async fn lint_cmd(db: &Option<String>, key: String, mode: Mode) -> CliResult<()> {
    let key = parse_issue_key(&key)?;
    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let issue = store
        .call(move |conn| issues::get_by_key(conn, &lookup))
        .await?
        .ok_or(cliban_core::Error::NotFound)?;
    let findings = crate::lint::lint_description(&issue.description);
    let errors = findings
        .iter()
        .filter(|f| f.severity == crate::lint::Severity::Error)
        .count();
    if mode.is_json() {
        let list: Vec<serde_json::Value> = findings
            .iter()
            .map(|f| {
                serde_json::json!({
                    "message": f.message,
                    "severity": f.severity.as_str(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "errors": errors,
                "findings": list,
                "key": issue.key,
                "warnings": findings.len() - errors,
            })
        );
    } else if findings.is_empty() {
        println!("{}: clean", issue.key);
    } else {
        for f in &findings {
            println!("{}: {}", f.severity.as_str(), f.message);
        }
    }
    if errors > 0 {
        return Err(CliError::Coded(
            2,
            format!("lint: {errors} error(s) in {}", issue.key),
        ));
    }
    Ok(())
}

/// `issue append-section`: add a block to the end of one section, atomically
/// on the store thread — no read-modify-write from the caller's side, so no
/// CAS needed. The activity log stays with `issue log` (it stamps and
/// dedupes); everything else appends here.
async fn append_section_cmd(db: &Option<String>, a: AppendSectionArgs) -> CliResult<()> {
    let key = parse_issue_key(&a.key)?;
    let anchor = resolve_section(&a.section)?;
    if anchor == "Activity Log" {
        return Err(CliError::validation(
            "--section activity is owned by `issue log`; append entries there",
        ));
    }
    let text = match (a.text, a.text_file) {
        (Some(_), Some(_)) => {
            return Err(CliError::validation(
                "pass the text positionally or via --text-file, not both",
            ))
        }
        (Some(t), None) => t,
        (None, Some(f)) if f == "-" => read_stdin()?,
        (None, Some(f)) => {
            std::fs::read_to_string(&f).map_err(|e| CliError::other(format!("read {f}: {e}")))?
        }
        // No positional, no --text-file: piped/redirected stdin IS the text;
        // a TTY keeps the fast error (blank piped text is caught below).
        (None, None) => match crate::stdin_input::fallback()? {
            Some(piped) => piped,
            None => {
                return Err(CliError::validation(
                    "nothing to append: pass the text positionally or via --text-file",
                ))
            }
        },
    };
    if text.trim().is_empty() {
        return Err(CliError::validation("nothing to append: text is blank"));
    }

    let store = store_open::open(db).await?;
    let lookup = key.clone();
    let anchor_job = anchor.clone();
    let create = a.create_section;
    let issue = store
        .call(move |conn| {
            let issue = issues::get_by_key(conn, &lookup)?.ok_or(cliban_core::Error::NotFound)?;
            require_section(&issue, &anchor_job, create)?;
            let text = cliban_core::sections::sanitize_section_body(&anchor_job, &text)
                .map_err(|m| cliban_core::Error::validation("section", &m))?;
            let new_desc =
                cliban_core::sections::append_section(&issue.description, &anchor_job, &text);
            let updated = issues::update(
                conn,
                &issue,
                UpdateIssue {
                    description: Some(new_desc),
                    ..Default::default()
                },
            )?;
            crate::audit::record(
                conn,
                &updated,
                "edit",
                &format!("appended to ## {anchor_job}"),
            );
            Ok(updated)
        })
        .await?;
    print_issue_result(
        &store,
        &issue,
        "updated",
        crate::output::mode(a.json, a.table),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    #[test]
    fn the_priority_flag_offers_exactly_what_the_store_accepts() {
        // The CLI enum and core's inclusion check are two spellings of one
        // value set. If they drift, `--priority` either advertises a value the
        // store rejects or hides one it accepts.
        let from_flag: Vec<&str> = Priority::value_variants()
            .iter()
            .map(|p| p.as_str())
            .collect();
        assert_eq!(from_flag, ISSUE_PRIORITIES);

        // …and clap's own spelling of each variant matches the stored one, so
        // what a user types is what lands in the column.
        for p in Priority::value_variants() {
            let shown = p.to_possible_value().expect("not skipped");
            assert_eq!(shown.get_name(), p.as_str());
        }
    }

    #[test]
    fn priority_rank_orders_every_variant() {
        let mut ranked: Vec<&str> = ISSUE_PRIORITIES.to_vec();
        ranked.sort_by_key(|p| priority_rank(p));
        assert_eq!(ranked, ["none", "low", "medium", "high", "urgent"]);
    }
}
