//! `cliban activity` — what changed on the board, newest first.
//!
//! Answers "what happened since yesterday?" without making the caller diff two
//! `issue ls` runs. Two kinds of event are merged into one time-ordered feed:
//!
//! - **state** — an issue was `created` and/or `completed` in the window, or
//!   failing both, `updated`. `updated` is strictly the fallback, so nothing
//!   is reported twice for one change.
//! - **recorded** — what cliban wrote down for itself as it worked: `status`
//!   moves, `archive`, field `edit`s, `plan` ticks and promotions, and the
//!   `log` notes an author wrote. Attributed to `$CLIBAN_ACTOR` when set.
//! - **log** (unrecorded) — a `## Activity Log` line with no matching record:
//!   hand-written, or predating automatic recording.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use chrono::{DateTime, Utc};
use cliban_core::contexts::issues::ListOpts;
use cliban_core::contexts::{activity_log, issues, milestones, projects};
use cliban_core::schema::{ActivityLogEntry, Issue};
use cliban_core::time::{format_usec, relative};

use crate::descmd;
use crate::errors::CliResult;
use crate::store_open;

/// Compact timestamp for the text feed — minute precision is plenty for
/// scanning, and it matches how `## Activity Log` entries are written.
const FEED_TIME_FORMAT: &str = "%Y-%m-%dT%H:%MZ";

/// Character budget for the trailing text column of the human feed.
const TEXT_WIDTH: usize = 110;

/// Truncate to `max` characters (not bytes), marking the cut.
fn elide(s: &str, max: usize) -> String {
    let flat = s.replace('\n', " ");
    if flat.chars().count() <= max {
        return flat;
    }
    flat.chars()
        .take(max.saturating_sub(1))
        .chain(['…'])
        .collect()
}

#[derive(clap::Args)]
pub struct ActivityArgs {
    /// how far back to look: 4h, 3d, 2w, today, yesterday, 2026-07-25, RFC3339
    #[arg(long, default_value = "1d")]
    since: String,
    /// project key filter
    #[arg(long)]
    project: Option<String>,
    /// milestone name filter
    #[arg(long)]
    milestone: Option<String>,
    /// include archived issues
    #[arg(long)]
    archived: bool,
    /// cap the number of events (0 = no cap)
    #[arg(long, default_value_t = 50)]
    limit: i64,
    /// NDJSON output (one compact JSON object per line)
    #[arg(long)]
    json: bool,
}

struct Event {
    ts: DateTime<Utc>,
    key: String,
    project: String,
    /// `created` | `completed` | `updated` for derived state events; for a
    /// recorded entry, whatever kind it was recorded under (`status`,
    /// `archive`, `edit`, `plan`, `log`).
    kind: String,
    /// The issue's status *now*, not at the time of the event — context for a
    /// log line, not a historical claim. Transitions are in `message`.
    status: String,
    title: String,
    milestone: Option<String>,
    /// Who did it, when `$CLIBAN_ACTOR` was set at the time. Only recorded
    /// entries carry this.
    actor: Option<String>,
    /// The log line's prose; `None` for state events.
    message: Option<String>,
}

impl Event {
    /// The trailing column: the prose (or title), prefixed with who did it.
    fn text(&self) -> String {
        let body = self.message.as_deref().unwrap_or(&self.title);
        match &self.actor {
            Some(a) => format!("[{a}] {body}"),
            None => body.to_string(),
        }
    }
}

pub async fn run(db: &Option<String>, a: ActivityArgs) -> CliResult<()> {
    let since = crate::since::parse(&a.since, "--since")?;
    let project = a.project.map(|p| p.to_uppercase());
    let store = store_open::open(db).await?;

    let (list_project, list_milestone, archived) =
        (project.clone(), a.milestone.clone(), a.archived);
    // The audit entries cliban recorded for itself, keyed by issue so the
    // per-issue merge below is a lookup rather than a scan.
    let recorded = store
        .call(move |conn| activity_log::list_since(conn, since))
        .await?;
    let mut by_issue: HashMap<i64, Vec<&ActivityLogEntry>> = HashMap::new();
    for entry in &recorded {
        by_issue.entry(entry.issue_id).or_default().push(entry);
    }

    let rows = store
        .call(move |conn| {
            // `ListOpts.archived` is an exact match, but `--archived` means
            // *include* archived here exactly as it does on `issue ls`, so
            // when set we fetch both sets and merge.
            let mut list = issues::list(
                conn,
                ListOpts {
                    project: list_project.as_deref(),
                    milestone: list_milestone.as_deref(),
                    archived: false,
                    ..Default::default()
                },
            )?;
            if archived {
                list.extend(issues::list(
                    conn,
                    ListOpts {
                        project: list_project.as_deref(),
                        milestone: list_milestone.as_deref(),
                        archived: true,
                        ..Default::default()
                    },
                )?);
            }
            // Resolve the display refs here, inside the one store call, rather
            // than issuing a lookup per issue from the async side.
            let mut out = Vec::with_capacity(list.len());
            for i in list {
                let project_key = projects::get_by_id(conn, i.project_id)?
                    .map(|p| p.key)
                    .unwrap_or_default();
                let milestone = match i.milestone_id {
                    Some(id) => milestones::get_by_id(conn, id)?.map(|m| m.name),
                    None => None,
                };
                out.push((i, project_key, milestone));
            }
            Ok(out)
        })
        .await?;

    let mut events = Vec::new();
    for (issue, project_key, milestone) in &rows {
        events.extend(events_for(
            issue,
            project_key,
            milestone.as_deref(),
            since,
            by_issue.get(&issue.id).map(Vec::as_slice).unwrap_or(&[]),
        ));
    }
    // Newest first; ties broken by key so the output is deterministic.
    events.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| a.key.cmp(&b.key)));
    if a.limit > 0 {
        events.truncate(a.limit as usize);
    }

    let now = Utc::now();
    if a.json {
        for e in &events {
            println!("{}", serde_json::to_string(&event_json(e)).unwrap());
        }
        return Ok(());
    }
    if events.is_empty() {
        println!("no activity since {}", a.since);
        return Ok(());
    }
    let key_width = events
        .iter()
        .map(|e| e.key.chars().count())
        .max()
        .unwrap_or(8)
        .max(8);
    for e in &events {
        println!(
            "{}  {:<key_width$}  {:<9}  {:<11}  {}",
            e.ts.format(FEED_TIME_FORMAT),
            e.key,
            e.kind,
            e.status,
            // Log prose runs long; the feed stays scannable and `--json`
            // carries the untruncated text.
            elide(&e.text(), TEXT_WIDTH),
        );
    }
    eprintln!(
        "— {} event(s) since {} ({})",
        events.len(),
        a.since,
        relative(since, now)
    );
    Ok(())
}

/// Every in-window event for one issue: its state change, the audit entries
/// cliban recorded, and the prose its author logged.
fn events_for(
    issue: &Issue,
    project: &str,
    milestone: Option<&str>,
    since: DateTime<Utc>,
    recorded: &[&ActivityLogEntry],
) -> Vec<Event> {
    let make = |ts: DateTime<Utc>, kind: &str, message: Option<String>| Event {
        ts,
        key: issue.key.clone(),
        project: project.to_string(),
        kind: kind.to_string(),
        status: issue.status.clone(),
        title: issue.title.clone(),
        milestone: milestone.map(str::to_string),
        actor: None,
        message,
    };

    let mut out = Vec::new();
    // `created` and `completed` are distinct facts and both get reported —
    // an issue opened and finished inside the window did both, and dropping
    // either misreads the window. `updated` is only the fallback for a change
    // neither of them already explains, so nothing double-counts.
    if issue.inserted_at >= since {
        out.push(make(issue.inserted_at, "created", None));
    }
    if let Some(done) = issue.completed_at.filter(|t| *t >= since) {
        out.push(make(done, "completed", None));
    }
    if out.is_empty() && issue.updated_at >= since {
        out.push(make(issue.updated_at, "updated", None));
    }
    // What cliban recorded on its own: status moves, archiving, field edits,
    // plan ticks and promotions. The recorded kind passes straight through so
    // a caller can filter on it.
    for entry in recorded {
        let mut e = make(entry.ts, &entry.kind, Some(entry.message.clone()));
        e.actor = crate::audit::actor_of(&entry.extra);
        out.push(e);
    }
    // …and the narrative its author wrote with `cliban issue log`. Those are
    // recorded in both places, so skip any markdown line that a recorded entry
    // already covers; what survives here is hand-written or pre-dates
    // recording.
    let already: std::collections::HashSet<(i64, String)> = recorded
        .iter()
        .filter(|e| e.kind == "log")
        .map(|e| crate::audit::log_dedupe_key(e.ts, &e.message))
        .collect();
    for (ts, message) in descmd::parse_activity_log(&issue.description) {
        if ts >= since && !already.contains(&crate::audit::log_dedupe_key(ts, &message)) {
            out.push(make(ts, "log", Some(message)));
        }
    }
    out
}

fn event_json(e: &Event) -> Value {
    let mut m = Map::new();
    m.insert(
        "actor".into(),
        match &e.actor {
            Some(s) => json!(s),
            None => Value::Null,
        },
    );
    m.insert("key".into(), json!(e.key));
    m.insert("kind".into(), json!(e.kind));
    m.insert(
        "message".into(),
        match &e.message {
            Some(s) => json!(s),
            None => Value::Null,
        },
    );
    m.insert(
        "milestone".into(),
        match &e.milestone {
            Some(s) => json!(s),
            None => Value::Null,
        },
    );
    m.insert("project".into(), json!(e.project));
    m.insert("issue_status".into(), json!(e.status));
    m.insert("title".into(), json!(e.title));
    m.insert("ts".into(), json!(format_usec(e.ts)));
    Value::Object(m)
}
