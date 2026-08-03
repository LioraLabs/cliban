//! `cliban import linear` and `cliban push linear` — the two verbs of the
//! Linear bridge.
//!
//! Both are explicit, one issue at a time, and neither runs in the background.
//! That is the whole design: there is no reconciliation loop to get wrong,
//! because the only time anything crosses the boundary is when someone asked
//! for it.
//!
//! What each side owns is fixed and not negotiable at runtime. Linear owns
//! title, priority, labels, due date, and workflow state. The `## Spec` prose
//! follows the link's recorded origin: a pairing created by `import` means
//! Linear wrote the spec first and a re-import refreshes it; a pairing created
//! by `push --create` means the spec was born on the board and a re-import
//! leaves it alone. cliban owns `## Plan`, `## Activity Log`, and `## Notes` —
//! an agent's half-ticked plan survives every refresh, which is the guarantee
//! that makes the bridge safe to run repeatedly.

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
    /// Linear issue key, e.g. ENG-412 (omit when using --mine)
    pub key: Option<String>,
    /// Import every open Linear issue assigned to the token's viewer: the
    /// active cycle where the team runs cycles, all open issues where it
    /// does not
    #[arg(long, conflicts_with_all = ["key", "link_to"])]
    pub mine: bool,
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

#[derive(clap::Args)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub cmd: SyncProvider,
}

#[derive(clap::Subcommand)]
pub enum SyncProvider {
    /// Refresh every Linear-linked issue on the board in one call
    Linear(SyncLinearArgs),
}

#[derive(clap::Args)]
pub struct SyncLinearArgs {
    /// Only refresh issues in this cliban project
    #[arg(long)]
    pub project: Option<String>,
    /// Report what would change and write nothing
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

pub async fn run_sync(db: &Option<String>, args: SyncArgs) -> CliResult<()> {
    match args.cmd {
        SyncProvider::Linear(a) => sync_linear(db, a).await,
    }
}

// ---------------------------------------------------------------- import

async fn import_linear(db: &Option<String>, args: ImportLinearArgs) -> CliResult<()> {
    if args.mine {
        return import_mine(db, args).await;
    }
    let Some(key) = args.key.as_deref() else {
        return Err(CliError::validation(
            "pass a Linear issue key (e.g. ENG-412), or --mine to import everything \
             assigned to you",
        ));
    };
    let (team_key, number) = ops::parse_issue_key(key).map_err(sync_err)?;
    let project = args.project.to_uppercase();

    let api = cliban_sync::linear::Client::from_env().map_err(sync_err)?;
    let remote = ops::issue_by_key(&api, &team_key, number)
        .await
        .map_err(sync_err)?;

    let store = store_open::open(db).await?;
    ensure_links_table(&store).await?;

    let mapped = states::to_cliban(&remote.state);

    if args.dry_run {
        let target = resolve_import_target(&store, &remote, args.link_to.as_deref()).await?;
        report_import_plan(&args, &remote, &mapped, &target);
        return Ok(());
    }

    let (issue, action) = import_one(
        &store,
        &project,
        args.milestone.clone(),
        args.link_to.as_deref(),
        &remote,
    )
    .await?;

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

/// One issue crossing the boundary inbound — the write path single-key
/// import, `--mine`, and `sync linear` all share. Resolves where the remote
/// lands, refreshes or creates honoring the link's origin, mirrors labels,
/// records the link, audits.
async fn import_one(
    store: &Store,
    project: &str,
    milestone: Option<String>,
    link_to: Option<&str>,
    remote: &model::Issue,
) -> CliResult<(Issue, &'static str)> {
    let mapped = states::to_cliban(&remote.state);
    let target = resolve_import_target(store, remote, link_to).await?;

    let (issue, action, origin) = match target {
        ImportTarget::Existing(existing, origin) => {
            let refreshed = refresh_existing(store, existing, remote, &mapped, origin).await?;
            (refreshed, "refreshed", origin)
        }
        // Adopting creates the pairing right now, via import — so its origin
        // is `Imported`, and this refresh installs Linear's spec.
        ImportTarget::Adopt(existing) => {
            let origin = links::Origin::Imported;
            let refreshed = refresh_existing(store, existing, remote, &mapped, origin).await?;
            (refreshed, "adopted", origin)
        }
        ImportTarget::Create => {
            let created = create_from_remote(store, project, remote, &mapped, milestone).await?;
            (created, "imported", links::Origin::Imported)
        }
    };

    let issue = apply_labels_and_link(store, issue, remote, origin).await?;
    audit(
        store,
        &issue,
        "sync",
        &format!("imported from {}", remote.identifier),
    )
    .await;
    Ok((issue, action))
}

/// The bookkeeping every inbound write ends with: mirror Linear's labels,
/// then record the link under the same origin the refresh honored.
async fn apply_labels_and_link(
    store: &Store,
    issue: Issue,
    remote: &model::Issue,
    origin: links::Origin,
) -> CliResult<Issue> {
    let labels = remote.label_names();
    let issue = if labels.is_empty() {
        issue
    } else {
        let issue_for_labels = issue.clone();
        store
            .call(move |conn| issues::set_labels(conn, &issue_for_labels, &labels))
            .await?
    };
    record_link(store, &issue, remote, origin).await?;
    Ok(issue)
}

/// `import linear --mine` — the inbound queue. Everything Linear says is
/// yours-and-current comes through [`import_one`], so a re-run refreshes what
/// the last run created and the whole verb is safe to run on a loop.
async fn import_mine(db: &Option<String>, args: ImportLinearArgs) -> CliResult<()> {
    let project = args.project.to_uppercase();

    let api = cliban_sync::linear::Client::from_env().map_err(sync_err)?;
    let assigned = ops::assigned_to_viewer(&api).await.map_err(sync_err)?;

    let store = store_open::open(db).await?;
    ensure_links_table(&store).await?;

    // A batch lands in one project; a typo'd key should fail before the first
    // write, not after the third.
    {
        let wanted = project.clone();
        store
            .call(move |conn| cliban_core::contexts::projects::get_by_key(conn, &wanted))
            .await?
            .ok_or_else(|| CliError::not_found(format!("not found: project {project}")))?;
    }

    let (mut created, mut refreshed, mut skipped) = (0u32, 0u32, 0u32);
    let mut rows = Vec::new();

    for a in &assigned {
        if !a.in_active_scope() {
            skipped += 1;
            rows.push(serde_json::json!({
                "action": "skipped",
                "linear": a.issue.identifier,
                "reason": "outside the team's active cycle",
            }));
            if !args.json {
                println!(
                    "skipped {} (outside the team's active cycle)",
                    a.issue.identifier
                );
            }
            continue;
        }

        if args.dry_run {
            let target = resolve_import_target(&store, &a.issue, None).await?;
            let would = match &target {
                ImportTarget::Existing(i, _) => format!("refresh {}", i.key),
                ImportTarget::Adopt(i) => format!("adopt {}", i.key),
                ImportTarget::Create => format!("create in {project}"),
            };
            match &target {
                ImportTarget::Create => created += 1,
                _ => refreshed += 1,
            }
            rows.push(serde_json::json!({
                "action": "dry-run",
                "linear": a.issue.identifier,
                "would": would,
            }));
            if !args.json {
                println!("dry run: would {} from {}", would, a.issue.identifier);
            }
            continue;
        }

        let (issue, action) =
            import_one(&store, &project, args.milestone.clone(), None, &a.issue).await?;
        match action {
            "imported" => created += 1,
            _ => refreshed += 1,
        }
        rows.push(serde_json::json!({
            "action": action,
            "cliban": issue.key,
            "linear": a.issue.identifier,
            "status": issue.status,
        }));
        if !args.json {
            println!("{action} {} → {}", a.issue.identifier, issue.key);
        }
    }

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "action": "import-mine",
                "dry_run": args.dry_run,
                "created": created,
                "refreshed": refreshed,
                "skipped": skipped,
                "issues": rows,
            })
        );
    } else {
        let prefix = if args.dry_run { "dry run: mine" } else { "mine" };
        println!("{prefix}: {created} created, {refreshed} refreshed, {skipped} skipped");
    }
    Ok(())
}

/// Where an import is going to land.
enum ImportTarget {
    /// Already linked — refresh in place, honoring the link's recorded origin.
    Existing(Issue, links::Origin),
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
        return Ok(ImportTarget::Existing(issue, link.origin));
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
    milestone: Option<String>,
) -> CliResult<Issue> {
    let project_owned = project.to_string();
    let attrs = issues::CreateIssue {
        title: remote.title.clone(),
        description: Some(render::initial_description(remote)),
        status: Some(mapped.status.to_string()),
        priority: Some(model::priority_to_cliban(remote.priority).to_string()),
        milestone,
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
///
/// Which side owns `## Spec` follows the link's origin: an imported pairing
/// means the spec came from Linear and refreshes with it; a pushed pairing
/// means the spec was written on the board, so a re-import leaves the whole
/// description alone and refreshes only the scalar fields Linear always owns.
async fn refresh_existing(
    store: &Store,
    issue: Issue,
    remote: &model::Issue,
    mapped: &states::Mapped,
    origin: links::Origin,
) -> CliResult<Issue> {
    warn_on_local_edits(store, &issue).await?;

    let description = match origin {
        links::Origin::Imported => render::refresh_description(&issue.description, remote),
        links::Origin::Pushed => issue.description.clone(),
    };
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
    let Some(link) = link else {
        return Ok(());
    };
    let Some(base) = link.base_hash else {
        return Ok(());
    };
    let labels = {
        let id = issue.id;
        store
            .call(move |conn| issues::label_names(conn, id))
            .await?
    };
    // The origin the base hash was recorded under: for a pushed link the spec
    // is cliban's, so it is outside both the hash and the field list.
    if local_fingerprint(issue, &labels, link.origin) != base {
        let owned = match link.origin {
            links::Origin::Imported => "title, priority, labels, due date, ## Spec",
            links::Origin::Pushed => "title, priority, labels, due date",
        };
        eprintln!(
            "warning: {} has local edits to fields Linear owns \
             ({owned}); refreshing overwrites them",
            issue.key
        );
    }
    Ok(())
}

/// Fingerprint of the remote-owned fields as they currently stand locally. At
/// import time this equals the fingerprint of what we just wrote, so a later
/// mismatch means someone edited the local copy.
///
/// `## Spec` counts as remote-owned only on an imported-origin link. On a
/// pushed-origin link the spec belongs to the board — editing it is normal
/// work, not drift, and it must not trip the overwrite warning.
fn local_fingerprint(issue: &Issue, labels: &[String], origin: links::Origin) -> String {
    let spec = match origin {
        links::Origin::Imported => {
            let (start, end, found) = sections::find_section(&issue.description, render::SPEC);
            if found {
                &issue.description[start..end]
            } else {
                ""
            }
        }
        links::Origin::Pushed => "",
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

// ---------------------------------------------------------------- sync

/// `cliban sync linear` — re-import every linked issue in one call.
///
/// Each link goes through exactly the semantics its origin recorded: an
/// imported pairing refreshes `## Spec` from Linear, a pushed pairing keeps
/// the board's spec; `## Plan`, `## Activity Log` and `## Notes` survive
/// unconditionally, which is what makes this safe to run on a loop.
async fn sync_linear(db: &Option<String>, args: SyncLinearArgs) -> CliResult<()> {
    let store = store_open::open(db).await?;
    ensure_links_table(&store).await?;

    // Resolve the project fence before touching anything remote.
    let project_id = match &args.project {
        Some(key) => {
            let key = key.to_uppercase();
            let wanted = key.clone();
            let project = store
                .call(move |conn| cliban_core::contexts::projects::get_by_key(conn, &wanted))
                .await?
                .ok_or_else(|| CliError::not_found(format!("not found: project {key}")))?;
            Some(project.id)
        }
        None => None,
    };

    let all_links = store
        .call(|conn| links::list(conn, PROVIDER_LINEAR))
        .await?;

    // Pair links with their local issues first: a board with nothing in scope
    // needs no token, and a dangling link should warn rather than abort.
    let mut skipped = 0u32;
    let mut rows = Vec::new();
    let mut work = Vec::new();
    for link in all_links {
        let local_id = link.local_id;
        let issue = store
            .call(move |conn| issues::get_by_id(conn, local_id))
            .await?;
        let Some(issue) = issue else {
            eprintln!(
                "warning: {} is linked to a cliban issue that no longer exists \
                 (local id {}); skipping",
                link.remote_key, link.local_id
            );
            skipped += 1;
            rows.push(serde_json::json!({
                "action": "skipped",
                "linear": link.remote_key,
                "reason": "local issue missing",
            }));
            continue;
        };
        if let Some(project_id) = project_id {
            if issue.project_id != project_id {
                continue;
            }
        }
        work.push((link, issue));
    }

    if work.is_empty() && skipped == 0 {
        if args.json {
            println!(
                "{}",
                serde_json::json!({
                    "action": "sync",
                    "dry_run": args.dry_run,
                    "refreshed": 0,
                    "skipped": 0,
                    "issues": [],
                })
            );
        } else {
            match &args.project {
                Some(p) => println!("nothing linked to Linear in {}", p.to_uppercase()),
                None => println!("nothing linked to Linear"),
            }
        }
        return Ok(());
    }

    let api = cliban_sync::linear::Client::from_env().map_err(sync_err)?;
    let mut refreshed = 0u32;

    for (link, issue) in work {
        let remote = match ops::issue_by_id(&api, &link.remote_id).await {
            Ok(remote) => remote,
            // Gone upstream. Deleting the local issue is not this command's
            // call to make — cliban never deletes — so say so and move on.
            Err(cliban_sync::Error::NotFound(_)) => {
                eprintln!(
                    "warning: {} ({}) no longer exists in Linear; skipping",
                    link.remote_key, issue.key
                );
                skipped += 1;
                rows.push(serde_json::json!({
                    "action": "skipped",
                    "cliban": issue.key,
                    "linear": link.remote_key,
                    "reason": "gone upstream",
                }));
                continue;
            }
            // Anything else (auth, network, API refusal) would fail for every
            // remaining link too: abort rather than warn N times.
            Err(e) => return Err(sync_err(e)),
        };

        if args.dry_run {
            refreshed += 1;
            rows.push(serde_json::json!({
                "action": "dry-run",
                "cliban": issue.key,
                "linear": remote.identifier,
            }));
            if !args.json {
                println!("dry run: would refresh {} ← {}", issue.key, remote.identifier);
            }
            continue;
        }

        let mapped = states::to_cliban(&remote.state);
        let issue = refresh_existing(&store, issue, &remote, &mapped, link.origin).await?;
        let issue = apply_labels_and_link(&store, issue, &remote, link.origin).await?;
        audit(
            &store,
            &issue,
            "sync",
            &format!("refreshed from {}", remote.identifier),
        )
        .await;
        refreshed += 1;
        rows.push(serde_json::json!({
            "action": "refreshed",
            "cliban": issue.key,
            "linear": remote.identifier,
            "status": issue.status,
        }));
        if !args.json {
            println!("refreshed {} ← {}", issue.key, remote.identifier);
        }
    }

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "action": "sync",
                "dry_run": args.dry_run,
                "refreshed": refreshed,
                "skipped": skipped,
                "issues": rows,
            })
        );
    } else {
        let prefix = if args.dry_run { "dry run: sync" } else { "sync" };
        println!("{prefix}: {refreshed} refreshed, {skipped} skipped");
    }
    Ok(())
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
    // The living comment is a snapshot, not a delta: the digest always renders
    // from the full activity log, whatever was pushed before, because the
    // comment it lands in replaces its own previous content.
    let activity = {
        let id = issue.id;
        store
            .call(move |conn| activity_log::list_for_issue(conn, id, 200))
            .await?
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
        render::progress_digest(&issue.key, &issue.status, plan_body.as_deref(), &activity)
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
    // One living comment per link, edited in place. `push` never appends: the
    // first push creates the comment, every later push rewrites it, and a
    // comment someone deleted in Linear is recreated once rather than errored.
    let comment_id = match &comment {
        None => None,
        Some(body) => {
            let existing = link.as_ref().and_then(|l| l.progress_comment_id.clone());
            match existing {
                Some(id) => match ops::update_comment(&api, &id, body).await {
                    Ok(()) => Some(id),
                    Err(e) if ops::comment_not_found(&e) => Some(
                        ops::create_comment(&api, &remote.id, body)
                            .await
                            .map_err(sync_err)?,
                    ),
                    Err(e) => return Err(sync_err(e)),
                },
                None => Some(
                    ops::create_comment(&api, &remote.id, body)
                        .await
                        .map_err(sync_err)?,
                ),
            }
        }
    };

    // A push that just created the Linear issue is the one place a pairing is
    // born with origin `Pushed` — the spec lived on the board first.
    let origin = match link.as_ref() {
        Some(link) => link.origin,
        None => links::Origin::Pushed,
    };
    record_link(&store, &issue, &remote, origin).await?;
    // After record_link, so the row exists even when this push just created
    // the pairing (--create). Recorded whenever a comment landed — on a
    // recreate this is exactly what swaps the dead id for the live one.
    if let Some(id) = comment_id {
        let local_id = issue.id;
        store
            .call(move |conn| {
                links::set_progress_comment(
                    conn,
                    PROVIDER_LINEAR,
                    ENTITY_ISSUE,
                    local_id,
                    Some(&id),
                )
            })
            .await?;
    }
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

// ---------------------------------------------------------------- push on move

/// `push_on_move = true` in linear.toml: after `issue mv` commits a status
/// change on a linked issue, push state + the living progress comment through
/// the same path as a manual `cliban push linear KEY` — stale-write guard,
/// status-mapping caveats, one-shot comment recreation and link bookkeeping
/// included, because it *is* that path with default writes.
///
/// Best-effort by design: the move has already committed, and nothing here may
/// take it back or fail the command. A push that cannot happen prints one
/// warning line and records a board activity entry, so nothing silently
/// drifts without anything becoming fatal.
///
/// Ordering is deliberate: the link is checked before the config is read, so a
/// move on an unlinked issue costs no config parse, no client, and no HTTP —
/// which is what most moves on most boards are.
pub async fn push_on_move(db: &Option<String>, key: &str) {
    let Ok(store) = store_open::open(db).await else {
        return;
    };
    if store.call(links::ensure_table).await.is_err() {
        return;
    }
    let wanted = key.to_string();
    let Ok(Some(issue)) = store
        .call(move |conn| issues::get_by_key(conn, &wanted))
        .await
    else {
        return;
    };
    let local_id = issue.id;
    let Ok(Some(_)) = store
        .call(move |conn| links::by_local(conn, PROVIDER_LINEAR, ENTITY_ISSUE, local_id))
        .await
    else {
        // Unlinked (or unknowable): nothing to push, and nothing further.
        return;
    };

    let attempt = async {
        let cfg = config::Config::load_default().map_err(sync_err)?;
        if !cfg.linear.push_on_move {
            return Ok(());
        }
        push_linear(
            db,
            PushLinearArgs {
                key: issue.key.clone(),
                create: false,
                team: None,
                state: false,
                comment: false,
                description: false,
                force: false,
                dry_run: false,
                json: false,
            },
        )
        .await
    };
    if let Err(e) = attempt.await {
        // One line however long the underlying message wants to be.
        let msg = e.message().split_whitespace().collect::<Vec<_>>().join(" ");
        eprintln!(
            "warning: push_on_move: {} moved locally but the push failed: {msg}",
            issue.key
        );
        audit(
            &store,
            &issue,
            "sync",
            &format!("push_on_move failed: {msg}"),
        )
        .await;
    }
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

/// `origin` is honored only when this call creates the pairing; on an existing
/// row the upsert keeps the recorded origin. It still matters on every call,
/// because the base hash must be computed under the same origin the next
/// `warn_on_local_edits` will read back.
async fn record_link(
    store: &Store,
    issue: &Issue,
    remote: &model::Issue,
    origin: links::Origin,
) -> CliResult<()> {
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
        base_hash: Some(local_fingerprint(&current, &labels, origin)),
        origin,
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
        ImportTarget::Existing(i, _) => format!("refresh {}", i.key),
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
    if matches!(target, ImportTarget::Existing(..) | ImportTarget::Adopt(_)) {
        println!("  ## Plan, ## Activity Log and ## Notes are preserved");
    }
    if matches!(target, ImportTarget::Existing(_, links::Origin::Pushed)) {
        println!("  ## Spec is preserved too (this pairing was created by push --create)");
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
