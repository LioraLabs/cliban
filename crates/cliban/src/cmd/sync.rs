//! `cliban import linear` and `cliban push linear` — the two verbs of the
//! Linear bridge.
//!
//! Both are explicit, one issue at a time, and neither runs in the background.
//! That is the whole design: there is no reconciliation loop to get wrong,
//! because the only time anything crosses the boundary is when someone asked
//! for it.
//!
//! What each side owns is fixed and not negotiable at runtime. Linear owns
//! title, priority, labels, due date, workflow state, and the `## Spec` prose.
//! cliban owns `## Plan`, `## Activity Log`, and `## Notes` — an agent's
//! half-ticked plan survives every refresh, which is the guarantee that makes
//! the bridge safe to run repeatedly.

use cliban_core::contexts::{activity_log, issues};
use cliban_core::schema::Issue;
use cliban_core::{sections, time, Store};
use cliban_sync::linear::{model, ops, render, states};
use cliban_sync::{config, links, ENTITY_ISSUE, PROVIDER_LINEAR};

use crate::errors::{CliError, CliResult};
use crate::store_open;

// ---------------------------------------------------------------- args

#[derive(clap::Args)]
pub struct ImportArgs {
    #[command(subcommand)]
    pub cmd: ImportProvider,
}

#[derive(clap::Subcommand)]
pub enum ImportProvider {
    /// Import (or refresh) a Linear issue as a cliban issue
    Linear(ImportLinearArgs),
}

#[derive(clap::Args)]
pub struct ImportLinearArgs {
    /// Linear issue key, e.g. ENG-412
    pub key: String,
    /// cliban project KEY the issue lands in
    #[arg(long)]
    pub project: String,
    /// Attach the imported issue to this milestone
    #[arg(long)]
    pub milestone: Option<String>,
    /// Adopt an existing cliban issue instead of creating one
    #[arg(long)]
    pub link_to: Option<String>,
    /// Report what would change and write nothing
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args)]
pub struct PushArgs {
    #[command(subcommand)]
    pub cmd: PushProvider,
}

#[derive(clap::Subcommand)]
pub enum PushProvider {
    /// Push a cliban issue's state and progress to its Linear counterpart
    Linear(PushLinearArgs),
}

#[derive(clap::Args)]
pub struct PushLinearArgs {
    /// cliban issue key, e.g. PROJ-42
    pub key: String,
    /// Create a Linear issue when this one is not linked yet
    #[arg(long)]
    pub create: bool,
    /// Team key for --create (default: [linear] team in linear.toml)
    #[arg(long)]
    pub team: Option<String>,
    /// Move the Linear issue's workflow state (default unless --comment or
    /// --description is given alone)
    #[arg(long)]
    pub state: bool,
    /// Post a progress comment
    #[arg(long)]
    pub comment: bool,
    /// Rewrite cliban's fenced block in the Linear description
    #[arg(long)]
    pub description: bool,
    /// Push even though Linear changed since the last sync
    #[arg(long)]
    pub force: bool,
    /// Report what would be written and write nothing
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub json: bool,
}

/// Which writes a push performs. With no flags at all, state and comment are
/// on and the description is not: the first two cannot destroy anything a human
/// wrote, and the third can.
struct Writes {
    state: bool,
    comment: bool,
    description: bool,
}

impl Writes {
    fn from(args: &PushLinearArgs) -> Self {
        if args.state || args.comment || args.description {
            return Self {
                state: args.state,
                comment: args.comment,
                description: args.description,
            };
        }
        Self {
            state: true,
            comment: true,
            description: false,
        }
    }

    fn names(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.state {
            out.push("state");
        }
        if self.comment {
            out.push("comment");
        }
        if self.description {
            out.push("description");
        }
        out
    }
}

// ---------------------------------------------------------------- dispatch

pub async fn run_import(db: &Option<String>, args: ImportArgs) -> CliResult<()> {
    match args.cmd {
        ImportProvider::Linear(a) => import_linear(db, a).await,
    }
}

pub async fn run_push(db: &Option<String>, args: PushArgs) -> CliResult<()> {
    match args.cmd {
        PushProvider::Linear(a) => push_linear(db, a).await,
    }
}

// ---------------------------------------------------------------- import

async fn import_linear(db: &Option<String>, args: ImportLinearArgs) -> CliResult<()> {
    let (team_key, number) = ops::parse_issue_key(&args.key).map_err(sync_err)?;
    let project = args.project.to_uppercase();

    let api = cliban_sync::linear::Client::from_env().map_err(sync_err)?;
    let remote = ops::issue_by_key(&api, &team_key, number)
        .await
        .map_err(sync_err)?;

    let store = store_open::open(db).await?;
    ensure_links_table(&store).await?;

    let mapped = states::to_cliban(&remote.state);
    let target = resolve_import_target(&store, &remote, args.link_to.as_deref()).await?;

    if args.dry_run {
        report_import_plan(&args, &remote, &mapped, &target);
        return Ok(());
    }

    let (issue, action) = match target {
        ImportTarget::Existing(existing) => {
            let refreshed = refresh_existing(&store, existing, &remote, &mapped).await?;
            (refreshed, "refreshed")
        }
        ImportTarget::Adopt(existing) => {
            let refreshed = refresh_existing(&store, existing, &remote, &mapped).await?;
            (refreshed, "adopted")
        }
        ImportTarget::Create => {
            let created = create_from_remote(&store, &project, &remote, &mapped, &args).await?;
            (created, "imported")
        }
    };

    let labels = remote.label_names();
    let issue = if labels.is_empty() {
        issue
    } else {
        let issue_for_labels = issue.clone();
        store
            .call(move |conn| issues::set_labels(conn, &issue_for_labels, &labels))
            .await?
    };

    record_link(&store, &issue, &remote).await?;
    audit(
        &store,
        &issue,
        "sync",
        &format!("imported from {}", remote.identifier),
    )
    .await;

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "action": action,
                "cliban": issue.key,
                "linear": remote.identifier,
                "status": issue.status,
                "archived": issue.archived,
                "url": remote.url,
            })
        );
    } else {
        println!("{action} {} → {}", remote.identifier, issue.key);
        if mapped.archive {
            println!(
                "  {} is cancelled in Linear; archived on the board",
                remote.identifier
            );
        }
    }
    Ok(())
}

/// Where an import is going to land.
enum ImportTarget {
    /// Already linked — refresh in place.
    Existing(Issue),
    /// `--link-to` named an unlinked issue to adopt.
    Adopt(Issue),
    Create,
}

async fn resolve_import_target(
    store: &Store,
    remote: &model::Issue,
    link_to: Option<&str>,
) -> CliResult<ImportTarget> {
    let remote_id = remote.id.clone();
    let existing_link = store
        .call(move |conn| links::by_remote(conn, PROVIDER_LINEAR, ENTITY_ISSUE, &remote_id))
        .await?;

    if let Some(link) = existing_link {
        let local_id = link.local_id;
        let issue = store
            .call(move |conn| issues::get_by_id(conn, local_id))
            .await?
            .ok_or_else(|| {
                // The link outlived the issue. Nothing deletes issues in normal
                // use, so this means someone went in with sqlite3.
                CliError::other(format!(
                    "{} is linked to a cliban issue that no longer exists (local id {})",
                    remote.identifier, local_id
                ))
            })?;
        if let Some(requested) = link_to {
            if !requested.eq_ignore_ascii_case(&issue.key) {
                return Err(CliError::validation(format!(
                    "{} is already linked to {}; --link-to {} would need two local issues \
                     pointing at one Linear issue",
                    remote.identifier, issue.key, requested
                )));
            }
        }
        return Ok(ImportTarget::Existing(issue));
    }

    match link_to {
        Some(key) => {
            let key = key.to_uppercase();
            let wanted = key.clone();
            let issue = store
                .call(move |conn| issues::get_by_key(conn, &wanted))
                .await?
                .ok_or_else(|| CliError::not_found(format!("not found: {key}")))?;
            Ok(ImportTarget::Adopt(issue))
        }
        None => Ok(ImportTarget::Create),
    }
}

async fn create_from_remote(
    store: &Store,
    project: &str,
    remote: &model::Issue,
    mapped: &states::Mapped,
    args: &ImportLinearArgs,
) -> CliResult<Issue> {
    let project_owned = project.to_string();
    let attrs = issues::CreateIssue {
        title: remote.title.clone(),
        description: Some(render::initial_description(remote)),
        status: Some(mapped.status.to_string()),
        priority: Some(model::priority_to_cliban(remote.priority).to_string()),
        milestone: args.milestone.clone(),
        parent_key: None,
        due_date: remote.due_date,
        position: None,
    };
    let created = store
        .call(move |conn| issues::create(conn, &project_owned, attrs))
        .await?;

    if mapped.archive {
        let target = created.clone();
        return Ok(store
            .call(move |conn| {
                issues::update(
                    conn,
                    &target,
                    issues::UpdateIssue {
                        archived: Some(true),
                        ..Default::default()
                    },
                )
            })
            .await?);
    }
    Ok(created)
}

/// Refresh the Linear-owned fields of an issue that is (or is about to be)
/// linked, preserving every cliban-owned description section.
async fn refresh_existing(
    store: &Store,
    issue: Issue,
    remote: &model::Issue,
    mapped: &states::Mapped,
) -> CliResult<Issue> {
    warn_on_local_edits(store, &issue).await?;

    let description = render::refresh_description(&issue.description, remote);
    let attrs = issues::UpdateIssue {
        title: Some(remote.title.clone()),
        description: Some(description),
        status: Some(mapped.status.to_string()),
        priority: Some(model::priority_to_cliban(remote.priority).to_string()),
        due_date: Some(remote.due_date),
        archived: if mapped.archive { Some(true) } else { None },
        ..Default::default()
    };
    let target = issue.clone();
    Ok(store
        .call(move |conn| issues::update(conn, &target, attrs))
        .await?)
}

/// Tell the user when a refresh is about to discard local edits to fields
/// Linear owns. Warn rather than refuse: ownership is declared, so the
/// overwrite is correct — it just should not be silent.
async fn warn_on_local_edits(store: &Store, issue: &Issue) -> CliResult<()> {
    let local_id = issue.id;
    let link = store
        .call(move |conn| links::by_local(conn, PROVIDER_LINEAR, ENTITY_ISSUE, local_id))
        .await?;
    let Some(base) = link.and_then(|l| l.base_hash) else {
        return Ok(());
    };
    let labels = {
        let id = issue.id;
        store
            .call(move |conn| issues::label_names(conn, id))
            .await?
    };
    if local_fingerprint(issue, &labels) != base {
        eprintln!(
            "warning: {} has local edits to fields Linear owns \
             (title, priority, labels, due date, ## Spec); refreshing overwrites them",
            issue.key
        );
    }
    Ok(())
}

/// Fingerprint of the remote-owned fields as they currently stand locally. At
/// import time this equals the fingerprint of what we just wrote, so a later
/// mismatch means someone edited the local copy.
fn local_fingerprint(issue: &Issue, labels: &[String]) -> String {
    let (start, end, found) = sections::find_section(&issue.description, render::SPEC);
    let spec = if found {
        &issue.description[start..end]
    } else {
        ""
    };
    let due = issue.due_date.map(time::format_date).unwrap_or_default();
    cliban_sync::fingerprint(&[
        &issue.title,
        &issue.priority,
        &due,
        &labels.join(","),
        spec.trim(),
    ])
}

// ---------------------------------------------------------------- push

async fn push_linear(db: &Option<String>, args: PushLinearArgs) -> CliResult<()> {
    let key = args.key.to_uppercase();
    let writes = Writes::from(&args);
    let cfg = config::Config::load_default().map_err(sync_err)?;

    let store = store_open::open(db).await?;
    ensure_links_table(&store).await?;

    let wanted = key.clone();
    let issue = store
        .call(move |conn| issues::get_by_key(conn, &wanted))
        .await?
        .ok_or_else(|| CliError::not_found(format!("not found: {key}")))?;

    let local_id = issue.id;
    let link = store
        .call(move |conn| links::by_local(conn, PROVIDER_LINEAR, ENTITY_ISSUE, local_id))
        .await?;

    // Checked before building a client: there is no reason to demand an API
    // token in order to tell someone their issue is not linked.
    if link.is_none() && !args.create {
        return Err(CliError::not_found(format!(
            "{} is not linked to Linear (pass --create to make a new Linear issue, \
             or `cliban import linear <KEY> --link-to {}` to adopt an existing one)",
            issue.key, issue.key
        )));
    }

    let api = cliban_sync::linear::Client::from_env().map_err(sync_err)?;

    let (remote, created) = match link.as_ref() {
        None => {
            let team_key = args
                .team
                .clone()
                .or_else(|| cfg.linear.team.clone())
                .ok_or_else(|| {
                    CliError::validation(
                        "--create needs a team: pass --team ENG or set `team` under \
                         [linear] in linear.toml",
                    )
                })?
                .to_uppercase();
            let team = ops::team_by_key(&api, &team_key).await.map_err(sync_err)?;
            let state = states::to_linear(&issue.status, &team.states.nodes, &cfg.linear.states);

            if args.dry_run {
                report_push_create_plan(&issue, &team_key, state.map(|s| s.name.as_str()), &writes);
                return Ok(());
            }

            let new = ops::NewIssue {
                team_id: team.id.clone(),
                title: issue.title.clone(),
                description: String::new(),
                state_id: state.map(|s| s.id.clone()),
                priority: model::priority_from_cliban(&issue.priority),
            };
            (ops::create_issue(&api, new).await.map_err(sync_err)?, true)
        }
        Some(link) => {
            let remote = ops::issue_by_id(&api, &link.remote_id)
                .await
                .map_err(sync_err)?;
            if !args.force {
                guard_stale_write(link, &remote)?;
            }
            (remote, false)
        }
    };

    // Work out every write before performing any, so --dry-run reports exactly
    // what a real run would do.
    let plan_body = plan_section(&issue);
    let activity = {
        let id = issue.id;
        let since = link.as_ref().map(|l| l.last_synced_at);
        let entries = store
            .call(move |conn| activity_log::list_for_issue(conn, id, 200))
            .await?;
        match since {
            Some(since) => entries.into_iter().filter(|e| e.ts > since).collect(),
            None => entries,
        }
    };

    let mut patch = ops::IssuePatch::default();
    let mut state_name = None;
    if writes.state {
        let team = ops::team_by_key(&api, &remote.team.key)
            .await
            .map_err(sync_err)?;
        match states::to_linear(&issue.status, &team.states.nodes, &cfg.linear.states) {
            Some(state) => {
                if state.id != remote.state.id {
                    patch.state_id = Some(state.id.clone());
                }
                state_name = Some(state.name.clone());
            }
            None => {
                eprintln!(
                    "warning: team {} has no workflow state for cliban status {:?}; \
                     leaving the Linear state alone",
                    remote.team.key, issue.status
                );
            }
        }
    }

    let comment = writes.comment.then(|| {
        render::progress_comment(&issue.key, &issue.status, plan_body.as_deref(), &activity)
    });

    if writes.description {
        let inner = render::progress_comment(&issue.key, &issue.status, plan_body.as_deref(), &[]);
        patch.description = Some(render::apply_fence(
            remote.description_text(),
            &issue.key,
            &inner,
        ));
    }

    if args.dry_run {
        report_push_plan(
            &issue,
            &remote,
            state_name.as_deref(),
            comment.as_deref(),
            &patch,
        );
        return Ok(());
    }

    let remote = if patch.is_empty() {
        remote
    } else {
        ops::update_issue(&api, &remote.id, patch)
            .await
            .map_err(sync_err)?
    };
    if let Some(body) = &comment {
        ops::create_comment(&api, &remote.id, body)
            .await
            .map_err(sync_err)?;
    }

    record_link(&store, &issue, &remote).await?;
    let verb = if created { "created" } else { "pushed" };
    audit(
        &store,
        &issue,
        "sync",
        &format!(
            "{verb} to {} ({})",
            remote.identifier,
            writes.names().join(", ")
        ),
    )
    .await;

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "action": verb,
                "cliban": issue.key,
                "linear": remote.identifier,
                "wrote": writes.names(),
                "state": state_name,
                "url": remote.url,
            })
        );
    } else {
        println!("{verb} {} → {}", issue.key, remote.identifier);
        for name in writes.names() {
            match name {
                "state" => println!(
                    "  state: {}",
                    state_name.as_deref().unwrap_or("(unchanged)")
                ),
                other => println!("  {other}: written"),
            }
        }
    }
    Ok(())
}

/// Refuse a push that would overwrite a Linear change we have not seen.
///
/// The comparison is against `remote_updated_at` recorded at the last sync, not
/// against wall-clock time — the question is "has anything happened upstream
/// since we last looked", and that is the only conflict detection the bridge
/// needs given that field ownership is fixed.
fn guard_stale_write(link: &links::RemoteLink, remote: &model::Issue) -> CliResult<()> {
    let Some(seen) = link.remote_updated_at else {
        return Ok(());
    };
    if remote.updated_at > seen {
        return Err(sync_err(cliban_sync::Error::StaleWrite {
            remote_key: remote.identifier.clone(),
            remote: time::format_usec(remote.updated_at),
            ours: time::format_usec(seen),
        }));
    }
    Ok(())
}

/// The `## Plan` body, or `None` when the issue has no plan section.
fn plan_section(issue: &Issue) -> Option<String> {
    let (start, end, found) = sections::find_section(&issue.description, render::PLAN);
    found.then(|| issue.description[start..end].to_string())
}

// ---------------------------------------------------------------- shared

async fn ensure_links_table(store: &Store) -> CliResult<()> {
    store.call(links::ensure_table).await?;
    Ok(())
}

async fn record_link(store: &Store, issue: &Issue, remote: &model::Issue) -> CliResult<()> {
    let labels = {
        let id = issue.id;
        store
            .call(move |conn| issues::label_names(conn, id))
            .await?
    };
    // Re-read the issue: refresh_existing rewrote the description, and the
    // fingerprint has to describe what is on disk now, not what was there when
    // the command started.
    let id = issue.id;
    let current = store
        .call(move |conn| issues::get_by_id(conn, id))
        .await?
        .unwrap_or_else(|| issue.clone());

    let new = links::NewLink {
        provider: PROVIDER_LINEAR.into(),
        entity: ENTITY_ISSUE.into(),
        local_id: issue.id,
        remote_id: remote.id.clone(),
        remote_key: remote.identifier.clone(),
        remote_updated_at: Some(remote.updated_at),
        base_hash: Some(local_fingerprint(&current, &labels)),
    };
    store.call(move |conn| links::upsert(conn, new)).await?;
    Ok(())
}

/// Record a timeline entry. Best-effort, matching `cliban_core::audit`: a sync
/// that already succeeded must not be reported as failed because its
/// bookkeeping row did not land.
async fn audit(store: &Store, issue: &Issue, kind: &str, message: &str) {
    let issue = issue.clone();
    let kind = kind.to_string();
    let message = message.to_string();
    let _ = store
        .call(move |conn| {
            cliban_core::audit::record(conn, &issue, &kind, &message);
            Ok(())
        })
        .await;
}

/// Map a bridge error onto cliban's exit codes: 1 not-found, 2 the user can fix
/// it, 3 everything else.
fn sync_err(e: cliban_sync::Error) -> CliError {
    use cliban_sync::Error as E;
    match &e {
        E::NotFound(_) => CliError::not_found(e.to_string()),
        E::MissingToken(_) | E::Unauthorized(_) | E::Config(_) | E::StaleWrite { .. } => {
            CliError::Coded(2, e.to_string())
        }
        E::Core(inner) => CliError::Coded(
            crate::errors::exit_code_for(inner),
            crate::errors::message_for(inner),
        ),
        _ => CliError::other(e.to_string()),
    }
}

// ---------------------------------------------------------------- dry-run output

fn report_import_plan(
    args: &ImportLinearArgs,
    remote: &model::Issue,
    mapped: &states::Mapped,
    target: &ImportTarget,
) {
    let action = match target {
        ImportTarget::Existing(i) => format!("refresh {}", i.key),
        ImportTarget::Adopt(i) => format!("adopt {}", i.key),
        ImportTarget::Create => format!("create a new issue in {}", args.project.to_uppercase()),
    };
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "dry_run": true,
                "action": action,
                "linear": remote.identifier,
                "title": remote.title,
                "status": mapped.status,
                "archive": mapped.archive,
                "priority": model::priority_to_cliban(remote.priority),
                "labels": remote.label_names(),
            })
        );
        return;
    }
    println!("dry run: would {action} from {}", remote.identifier);
    println!("  title:    {}", remote.title);
    println!(
        "  status:   {} (Linear: {})",
        mapped.status, remote.state.name
    );
    println!("  priority: {}", model::priority_to_cliban(remote.priority));
    if !remote.label_names().is_empty() {
        println!("  labels:   {}", remote.label_names().join(", "));
    }
    if mapped.archive {
        println!("  archived: yes (cancelled in Linear)");
    }
    if matches!(target, ImportTarget::Existing(_) | ImportTarget::Adopt(_)) {
        println!("  ## Plan, ## Activity Log and ## Notes are preserved");
    }
}

fn report_push_create_plan(
    issue: &Issue,
    team_key: &str,
    state_name: Option<&str>,
    writes: &Writes,
) {
    println!(
        "dry run: would create a Linear issue in {team_key} for {}",
        issue.key
    );
    println!("  title: {}", issue.title);
    println!("  state: {}", state_name.unwrap_or("(team default)"));
    println!("  then write: {}", writes.names().join(", "));
}

fn report_push_plan(
    issue: &Issue,
    remote: &model::Issue,
    state_name: Option<&str>,
    comment: Option<&str>,
    patch: &ops::IssuePatch,
) {
    println!("dry run: would push {} → {}", issue.key, remote.identifier);
    match (&patch.state_id, state_name) {
        (Some(_), Some(name)) => println!("  state: {} → {name}", remote.state.name),
        (None, Some(name)) => println!("  state: already {name}, no change"),
        _ => {}
    }
    if let Some(body) = comment {
        println!("  comment:");
        for line in body.lines() {
            println!("    {line}");
        }
    }
    if patch.description.is_some() {
        println!("  description: cliban's fenced block would be rewritten");
        println!("               (prose outside the fence is untouched)");
    }
    if patch.state_id.is_none() && comment.is_none() && patch.description.is_none() {
        println!("  nothing to write");
    }
}
