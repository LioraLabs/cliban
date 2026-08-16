//! `cliban milestone` subcommands.

use chrono::{NaiveDate, Utc};
use rusqlite::{Transaction, TransactionBehavior};
use serde_json::{json, Map, Value};

use cliban_core::contexts::issues::ListOpts;
use cliban_core::contexts::milestones::{CreateMilestone, UpdateMilestone};
use cliban_core::contexts::{issues, milestones};
use cliban_core::schema::Milestone;
use cliban_core::time::{format_date, format_usec, parse_date};
use cliban_core::Store;

use crate::errors::{CliError, CliResult};
use crate::output::{Detail, Mode};
use crate::store_open;

async fn resolve_name(store: &Store, project: &str, input: &str, mode: Mode) -> CliResult<String> {
    let list_project = project.to_string();
    let milestones = store
        .call(move |conn| milestones::list(conn, Some(&list_project)))
        .await?;

    if let Some(m) = milestones.iter().find(|m| m.name == input) {
        return Ok(m.name.clone());
    }

    let folded: Vec<_> = milestones
        .iter()
        .filter(|m| m.name.eq_ignore_ascii_case(input))
        .collect();
    if folded.len() == 1 {
        return Ok(folded[0].name.clone());
    }

    let mut matches: Vec<_> = milestones
        .iter()
        .filter_map(|m| crate::search::fuzzy_find(input, &m.name).map(|(score, _)| (score, m)))
        .collect();
    matches.sort_by(|(a, am), (b, bm)| b.cmp(a).then_with(|| am.name.cmp(&bm.name)));
    if matches.len() == 1
        && matches[0].0 > 0
        && input.chars().count() * 2 >= matches[0].1.name.chars().count()
    {
        return Ok(matches[0].1.name.clone());
    }

    let candidates: Vec<&str> = if matches.is_empty() {
        milestones.iter().take(5).map(|m| m.name.as_str()).collect()
    } else {
        matches
            .iter()
            .take(5)
            .map(|(_, m)| m.name.as_str())
            .collect()
    };
    if mode.is_json() {
        for name in &candidates {
            println!("{}", json!({"name": name, "project": project}));
        }
    }
    let suffix = if candidates.is_empty() || mode.is_json() {
        String::new()
    } else {
        format!("; candidates: {}", candidates.join(", "))
    };
    Err(CliError::not_found(format!("not found: {input}{suffix}")))
}

#[derive(clap::Args)]
pub struct MilestoneArgs {
    #[command(subcommand)]
    pub cmd: MilestoneCmd,
}

#[derive(clap::Subcommand)]
pub enum MilestoneCmd {
    /// Partition a milestone's open issues into dependency waves (wave N is
    /// safe to start once waves 1..N-1 are done)
    Waves {
        /// milestone name
        name: String,
        /// project key (default: $CLIBAN_PROJECT)
        #[arg(long, short = 'p')]
        project: Option<String>,
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Add a milestone
    Add {
        /// milestone name
        name: String,
        /// project key (default: $CLIBAN_PROJECT)
        #[arg(long, short = 'p')]
        project: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        description: Option<String>,
        #[arg(long = "description-file")]
        description_file: Option<String>,
        /// target date YYYY-MM-DD
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// List milestones
    Ls {
        /// project key (default: $CLIBAN_PROJECT); unscoped (-p '*' or no
        /// scope at all) is a per-project summary — counts only, no rows
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// filter by status (open|completed|cancelled)
        #[arg(long, short = 's')]
        status: Option<String>,
        /// order: activity (most recently worked on) | name | target
        #[arg(long, default_value = "name")]
        sort: String,
        /// add done/total progress and last-activity columns
        #[arg(long)]
        stats: bool,
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
        /// full rows in --json output: description body plus the fields list rows omit
        #[arg(long)]
        full: bool,
    },
    /// Show a milestone
    Show {
        /// milestone name
        name: String,
        /// project key (default: $CLIBAN_PROJECT)
        #[arg(long, short = 'p')]
        project: Option<String>,
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Edit a milestone
    Edit {
        /// milestone name
        name: String,
        /// project key (default: $CLIBAN_PROJECT)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// new name
        #[arg(long = "name")]
        rename: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        description: Option<String>,
        #[arg(long = "description-file")]
        description_file: Option<String>,
        /// new status (open|completed|cancelled)
        #[arg(long, short = 's')]
        status: Option<String>,
        /// new target date YYYY-MM-DD
        #[arg(long)]
        target: Option<String>,
        /// clear target date
        #[arg(long = "clear-target")]
        clear_target: bool,
        /// JSON output (echo the updated milestone)
        #[arg(long)]
        json: bool,
        /// human output (one-line confirmation)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Append an entry to the milestone's ## Activity Log section
    Log {
        /// milestone name
        name: String,
        /// log message (leading "-" is fine)
        #[arg(allow_hyphen_values = true)]
        message: Option<String>,
        /// read message from file (use '-' for stdin)
        #[arg(long = "message-file")]
        message_file: Option<String>,
        /// project key (default: $CLIBAN_PROJECT)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// A unix reflex, not a real deleter (hidden): cancels, says so, names
    /// the undo.
    #[command(hide = true)]
    Rm {
        /// milestone name
        name: String,
        /// project key (default: $CLIBAN_PROJECT)
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

pub async fn run(db: &Option<String>, args: MilestoneArgs) -> CliResult<()> {
    match args.cmd {
        MilestoneCmd::Waves {
            project,
            name,
            json,
            table,
        } => waves(db, project, name, crate::output::mode(json, table)).await,
        MilestoneCmd::Add {
            project,
            name,
            description,
            description_file,
            target,
            json,
            table,
        } => {
            add(
                db,
                project,
                name,
                description,
                description_file,
                target,
                crate::output::mode(json, table),
            )
            .await
        }
        MilestoneCmd::Ls {
            project,
            status,
            sort,
            stats,
            json,
            table,
            full,
        } => {
            ls(
                db,
                project,
                status,
                sort,
                stats,
                crate::output::mode(json, table),
                full,
            )
            .await
        }
        MilestoneCmd::Show {
            name,
            project,
            json,
            table,
        } => show(db, name, project, crate::output::mode(json, table), json).await,
        MilestoneCmd::Edit {
            project,
            name,
            rename,
            description,
            description_file,
            status,
            target,
            clear_target,
            json,
            table,
        } => {
            let project_key = crate::scope::required_project(project)?;
            let mode = crate::output::mode(json, table);
            let (m, count) = apply_edit(
                db,
                project_key.clone(),
                name,
                rename,
                description,
                description_file,
                status,
                target,
                clear_target,
                mode,
            )
            .await?;
            if mode.is_json() {
                let v = milestone_json(&m, &project_key, count, Detail::Echo);
                println!("{}", serde_json::to_string(&v).unwrap());
            } else {
                println!("updated milestone {} in {}", m.name, project_key);
            }
            Ok(())
        }
        MilestoneCmd::Log {
            name,
            message,
            message_file,
            project,
            json,
            table,
        } => {
            log(
                db,
                project,
                name,
                message,
                message_file,
                crate::output::mode(json, table),
            )
            .await
        }
        MilestoneCmd::Rm { project, name } => {
            // `cancelled` is a milestone's archived state, so that is what a
            // delete becomes — the closest safe thing, not a refusal. The
            // message IS the point — it prints in every mode.
            let project_key = crate::scope::required_project(project)?;
            apply_edit(
                db,
                project_key.clone(),
                name.clone(),
                None,
                None,
                None,
                Some("cancelled".to_string()),
                None,
                false,
                Mode::Table,
            )
            .await?;
            println!(
                "cancelled milestone {name} in {project_key} — cliban cancels instead of \
                 deleting (undo: cliban milestone edit {name:?} -p {project_key} --status open)"
            );
            Ok(())
        }
    }
}

/// Parse `--target`: empty → None; otherwise `YYYY-MM-DD`. A parse failure
/// is a plain exit-3 error, deliberately not a validation error.
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

/// `resolveDescription`: returns `(content, was_set)` over the shared
/// resolver, which owns the mutual exclusion and the `-` sentinel.
fn resolve_description(
    description: Option<String>,
    description_file: Option<String>,
) -> CliResult<(String, bool)> {
    let resolved = crate::stdin_input::resolve(
        description,
        description_file,
        "--description",
        "--description-file",
    )?;
    Ok(resolved.map_or_else(|| (String::new(), false), |s| (s, true)))
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

fn milestone_json(m: &Milestone, project: &str, count: i64, detail: Detail) -> Value {
    crate::output::build_milestone_json(
        &m.name,
        Some(project.to_string()),
        &m.description,
        m.target_date.map(format_date),
        &m.status,
        &format_usec(m.inserted_at),
        &format_usec(m.updated_at),
        count,
        detail,
    )
}

async fn add(
    db: &Option<String>,
    project: Option<String>,
    name: String,
    description: Option<String>,
    description_file: Option<String>,
    target: Option<String>,
    mode: Mode,
) -> CliResult<()> {
    let project_key = crate::scope::required_project(project)?;
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

    if mode.is_json() {
        let count = issue_count(&store, project_key.clone(), m.name.clone()).await?;
        let v = milestone_json(&m, &project_key, count, Detail::Echo);
        println!("{}", serde_json::to_string(&v).unwrap());
    } else {
        println!("created milestone {} in {}", m.name, project_key);
    }
    Ok(())
}

async fn ls(
    db: &Option<String>,
    project: Option<String>,
    status: Option<String>,
    sort: String,
    stats: bool,
    mode: Mode,
    full: bool,
) -> CliResult<()> {
    let project_key = crate::scope::project(project);
    let sort = milestones::Sort::parse(&sort).ok_or_else(|| {
        CliError::validation(format!(
            "invalid --sort {sort:?} (want activity, name or target)"
        ))
    })?;
    let status = status.filter(|s| !s.is_empty());

    // Unscoped, milestone rows would span every project on the machine —
    // 90 rows on a real box, none of which the caller can address without
    // knowing the project anyway (milestones are named per project). So the
    // unscoped form answers the only question it can: which projects have
    // milestones, and how many. Detail flags without a scope are a mistake
    // worth naming, not silently reinterpreting.
    if project_key.is_none() {
        if stats || full || status.is_some() {
            return Err(CliError::validation(
                "--stats/--full/--status need --project; unscoped milestone ls \
                 is a per-project summary",
            ));
        }
        return ls_summary(db, mode).await;
    }

    let store = store_open::open(db).await?;
    let (key, st) = (project_key.clone(), status.clone());
    let ms = store
        .call(move |conn| {
            milestones::summaries(
                conn,
                milestones::SummaryOpts {
                    project: key.as_deref(),
                    status: st.as_deref(),
                    sort,
                },
            )
        })
        .await?;

    let now = cliban_core::time::now_usec();
    if mode.is_json() {
        for s in &ms {
            let v = if stats {
                milestone_stats_json(s, now)
            } else {
                milestone_json(
                    &s.milestone,
                    &s.project_key,
                    s.total,
                    Detail::from_full_flag(full),
                )
            };
            println!("{}", serde_json::to_string(&v).unwrap());
        }
        return Ok(());
    }

    // The scoped, no-stats form stays byte-identical to the original three
    // columns; the project column only appears when the listing spans
    // projects, and the rollups only when asked for. With `--stats` the name
    // column is sized to the data (real milestone names run well past 15
    // chars) so the numeric columns stay in line.
    let name_width = ms
        .iter()
        .map(|s| s.milestone.name.chars().count())
        .max()
        .unwrap_or(15)
        .clamp(15, 44);
    for s in &ms {
        let m = &s.milestone;
        let tgt = m
            .target_date
            .map(format_date)
            .unwrap_or_else(|| "-".to_string());
        let mut line = String::new();
        if project_key.is_none() {
            line.push_str(&format!("{:<8} ", s.project_key));
        }
        if stats {
            // Pad the (variable-width) target so the rollup columns line up.
            line.push_str(&format!(
                "{:<name_width$} {:<10} {:<12} {:>7}  {}",
                m.name,
                m.status,
                tgt,
                format!("{}/{}", s.done, s.total),
                cliban_core::time::relative(s.last_activity, now)
            ));
        } else {
            line.push_str(&format!("{:<15} {:<10} {}", m.name, m.status, tgt));
        }
        println!("{}", line.trim_end());
    }
    Ok(())
}

/// The unscoped form: one row per project that has milestones —
/// `{"milestones":N,"open":N,"project":KEY}` — nothing an agent has to pay
/// for and then not read. Projects without milestones don't appear.
async fn ls_summary(db: &Option<String>, mode: Mode) -> CliResult<()> {
    let store = store_open::open(db).await?;
    let ms = store
        .call(move |conn| {
            milestones::summaries(
                conn,
                milestones::SummaryOpts {
                    project: None,
                    status: None,
                    sort: milestones::Sort::Name,
                },
            )
        })
        .await?;
    let mut by_project: std::collections::BTreeMap<String, (i64, i64)> =
        std::collections::BTreeMap::new();
    for s in &ms {
        let e = by_project.entry(s.project_key.clone()).or_insert((0, 0));
        e.0 += 1;
        if s.milestone.status == "open" {
            e.1 += 1;
        }
    }
    for (project, (total, open)) in &by_project {
        if mode.is_json() {
            let mut m = Map::new();
            m.insert("milestones".into(), json!(total));
            m.insert("open".into(), json!(open));
            m.insert("project".into(), json!(project));
            println!("{}", serde_json::to_string(&Value::Object(m)).unwrap());
        } else {
            println!("{project:<8} {total} milestone(s), {open} open");
        }
    }
    Ok(())
}

/// `--stats` JSON: the lean list row plus the rollups. A stats row is still a
/// list row, so it follows the Brief contract — no `description` body, no
/// `created_at`, absent `target_date` when unset, second-precision stamps.
fn milestone_stats_json(
    s: &milestones::MilestoneSummary,
    now: chrono::DateTime<chrono::Utc>,
) -> Value {
    let m = &s.milestone;
    let mut map = Map::new();
    map.insert("done_count".into(), json!(s.done));
    map.insert("issue_count".into(), json!(s.total));
    map.insert(
        "last_activity".into(),
        json!(crate::output::trim_usec(&format_usec(s.last_activity))),
    );
    map.insert(
        "last_activity_human".into(),
        json!(cliban_core::time::relative(s.last_activity, now)),
    );
    map.insert("name".into(), json!(m.name));
    map.insert("project".into(), json!(s.project_key));
    map.insert("status".into(), json!(m.status));
    if let Some(t) = m.target_date.map(format_date) {
        map.insert("target_date".into(), json!(t));
    }
    map.insert(
        "updated_at".into(),
        json!(crate::output::trim_usec(&format_usec(m.updated_at))),
    );
    Value::Object(map)
}

async fn show(
    db: &Option<String>,
    name: String,
    project: Option<String>,
    mode: Mode,
    explicit_json: bool,
) -> CliResult<()> {
    let project_key = crate::scope::required_project(project)?;

    let store = store_open::open(db).await?;
    let resolved = resolve_name(&store, &project_key, &name, mode).await?;
    let get_key = project_key.clone();
    let get_name = resolved.clone();
    let m = store
        .call(move |conn| milestones::get(conn, &get_key, &get_name))
        .await?
        .ok_or_else(|| CliError::not_found(format!("not found: {resolved}")))?;

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

    if mode.is_json() {
        let value = crate::output::build_milestone_json(
            &m.name,
            Some(project_key),
            &m.description,
            m.target_date.map(format_date),
            &m.status,
            &format_usec(m.inserted_at),
            &format_usec(m.updated_at),
            count,
            crate::output::single_detail(explicit_json),
        );
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
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
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn apply_edit(
    db: &Option<String>,
    project_key: String,
    name: String,
    rename: Option<String>,
    description: Option<String>,
    description_file: Option<String>,
    status: Option<String>,
    target: Option<String>,
    clear_target: bool,
    mode: Mode,
) -> CliResult<(Milestone, i64)> {
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
    let name = resolve_name(&store, &project_key, &name, mode).await?;
    let call_key = project_key.clone();
    let m = store
        .call(move |conn| {
            // BEGIN IMMEDIATE: read-modify-write (CLI-88).
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let cur = milestones::get(&tx, &call_key, &name)?
                .ok_or_else(|| cliban_core::Error::NamedNotFound(name.clone()))?;
            // Unlike an issue, a milestone has no durable rows behind its
            // ## Activity Log — the section is the only copy of the record,
            // including the INTEGRATED ledger. A full-replace that loses it
            // is refused; deliberate deletion means writing the anchor
            // explicitly in the replacement.
            if let Some(new_desc) = params.description.as_ref() {
                if cliban_core::sections::find_section(&cur.description, "Activity Log").2
                    && !cliban_core::sections::find_section(new_desc, "Activity Log").2
                {
                    return Err(cliban_core::Error::validation(
                        "description",
                        "the replacement drops ## Activity Log — the milestone's only record; \
                         carry the section over in the new description (append entries with \
                         `milestone log`, never a rewrite)",
                    ));
                }
            }
            let out = milestones::update(&tx, &cur, params)?;
            tx.commit()?;
            Ok(out)
        })
        .await?;
    let count = issue_count(&store, project_key, m.name.clone()).await?;
    Ok((m, count))
}

/// `milestone log`: append one timestamped entry to the milestone's
/// `## Activity Log`, creating the section when it is absent. The
/// milestone-scoped sibling of `issue log`, and the only place an action that
/// is about the milestone rather than about one ticket — a milestone being
/// set up, a landing attempted — has to record itself. The line it writes is
/// built by the same `descmd::append_activity_log` the issue side uses, so one
/// parser reads both.
///
/// The append is a splice inside a transaction, never a `--description`
/// rewrite. `milestone edit --description` is full-replace, and a milestone
/// description is exactly the document several agents record against at once:
/// a rewrite drops whatever landed between one writer's read and its write.
///
/// The transaction is `IMMEDIATE` rather than the deferred one `issue log`
/// opens. A deferred transaction reads first and only then tries to upgrade
/// its snapshot to a write; when another process wrote in between, SQLite
/// answers `SQLITE_BUSY_SNAPSHOT`, which the connection's `busy_timeout` does
/// not retry — measured on this board as 7 of 12 concurrent `issue log`
/// processes failing outright rather than queueing. Taking the write lock at
/// `BEGIN` moves the contention into the class `busy_timeout` does retry, so
/// concurrent writers wait their turn instead of losing it. Nothing here reads
/// before the lock is held, so there is no cost to paying for it up front.
async fn log(
    db: &Option<String>,
    project: Option<String>,
    name: String,
    message: Option<String>,
    message_file: Option<String>,
    mode: Mode,
) -> CliResult<()> {
    let project_key = crate::scope::required_project(project)?;
    let msg = crate::stdin_input::log_message(message, message_file)?;

    let store = store_open::open(db).await?;
    let name = resolve_name(&store, &project_key, &name, mode).await?;
    let now = Utc::now();
    let (call_key, call_name, entry) = (project_key.clone(), name.clone(), msg.clone());
    store
        .call(move |conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let cur = milestones::get(&tx, &call_key, &call_name)?
                .ok_or_else(|| cliban_core::Error::NamedNotFound(call_name.clone()))?;
            let new_desc = crate::descmd::append_activity_log(&cur.description, &entry, now)
                .map_err(|m| cliban_core::Error::Validation(vec![("message".into(), m)]))?;
            milestones::update(
                &tx,
                &cur,
                UpdateMilestone {
                    description: Some(new_desc),
                    ..Default::default()
                },
            )?;
            tx.commit()?;
            Ok(())
        })
        .await?;

    if mode.is_json() {
        println!(
            "{}",
            json!({
                "entry": msg,
                "milestone": name,
                "project": project_key,
                "timestamp": format_usec(now),
            })
        );
    } else {
        println!("logged on milestone {name} in {project_key}: {msg}");
    }
    Ok(())
}

/// `milestone waves`: print the dependency-wave partition the core computes —
/// the deterministic answer to "what can run in parallel, in what order" that
/// orchestrators previously derived by hand from the blocking graph.
async fn waves(
    db: &Option<String>,
    project: Option<String>,
    name: String,
    mode: Mode,
) -> CliResult<()> {
    let project = crate::scope::required_project(project)?;
    let store = store_open::open(db).await?;
    let name = resolve_name(&store, &project, &name, mode).await?;
    let w = store
        .call(move |conn| milestones::waves(conn, &project, &name))
        .await?;
    if mode.is_json() {
        println!(
            "{}",
            json!({
                "chains": w.chains,
                "done": w.done,
                "external_blocked": w.external_blocked,
                "waves": w.waves,
            })
        );
        return Ok(());
    }
    if w.waves.is_empty() && w.external_blocked.is_empty() {
        println!("no open issues");
    }
    for (i, wave) in w.waves.iter().enumerate() {
        println!("wave {}: {}", i + 1, wave.join(", "));
    }
    if !w.external_blocked.is_empty() {
        println!("externally blocked: {}", w.external_blocked.join(", "));
    }
    if !w.done.is_empty() {
        println!("done: {}", w.done.join(", "));
    }
    if !w.chains.is_empty() {
        println!(
            "chains: {}",
            w.chains
                .iter()
                .map(|c| format!("[{}]", c.join(", ")))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    Ok(())
}
