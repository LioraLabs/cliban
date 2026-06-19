//! `cliban fff` — fuzzy issue picker. In an interactive TTY this would drive a
//! Bubble Tea-style picker (out of scope, CLI-8); in any non-TTY context
//! (tests, pipes) it runs "batch mode": require a QUERY, search, and emit one
//! NDJSON search-match line per result. Output is byte-for-byte parity with the
//! Go oracle (`internal/cli/fff.go`).

use std::io::IsTerminal;

use crate::errors::{CliError, CliResult};
use crate::store_open;

#[derive(clap::Args)]
pub struct FffArgs {
    /// optional initial query
    query: Option<String>,
    /// project key filter
    #[arg(long)]
    project: Option<String>,
    /// label name filter
    #[arg(long)]
    label: Option<String>,
    /// milestone filter
    #[arg(long)]
    milestone: Option<String>,
    /// status filter
    #[arg(long)]
    status: Option<String>,
    /// priority filter
    #[arg(long)]
    priority: Option<String>,
    /// list sub-issues of this parent key
    #[arg(long)]
    parent: Option<String>,
    /// include archived issues
    #[arg(long)]
    archived: bool,
    /// exclude sub-issues
    #[arg(long = "no-subs")]
    no_subs: bool,
    /// after picking, open `issue show` for the selection (v1.1 — stubbed)
    #[arg(long, hide = true)]
    show: bool,
    /// after picking, open `issue edit --editor` for the selection (v1.1 — stubbed)
    #[arg(long, hide = true)]
    edit: bool,
    /// emit full issue JSON instead of just the key (v1.1 — stubbed)
    #[arg(long, hide = true)]
    json: bool,
}

pub async fn run(db: &Option<String>, a: FffArgs) -> CliResult<()> {
    // At most one of --show, --edit, --json.
    let modes = [a.show, a.edit, a.json].iter().filter(|b| **b).count();
    if modes > 1 {
        return Err(CliError::validation(
            "--show, --edit, and --json are mutually exclusive",
        ));
    }

    let query = a.query.clone().unwrap_or_default();

    if std::io::stdin().is_terminal() {
        // Interactive picker path — loom-TUI work (CLI-8), out of scope.
        return Err(CliError::other("fff interactive picker not yet wired"));
    }

    // Batch mode (tests, pipes). Query is required.
    if query.trim().is_empty() {
        return Err(CliError::validation(
            "cliban fff in non-interactive mode requires a QUERY",
        ));
    }

    let store = store_open::open(db).await?;
    let opts = crate::search::Options {
        query,
        project: a.project.as_deref().map(str::to_uppercase),
        label: a.label.clone().into_iter().collect(),
        milestone: a.milestone.clone(),
        status: a.status.clone(),
        priority: a.priority.clone(),
        parent: a.parent.clone(),
        include_archived: a.archived,
        exclude_subs: a.no_subs,
        limit: 50,
    };
    let matches = crate::search::search(&store, opts).await?;
    for m in &matches {
        let inputs = crate::cmd::issue::issue_json_inputs(&store, &m.issue).await?;
        println!(
            "{}",
            serde_json::to_string(&crate::output::build_search_match_json(inputs, m.score)).unwrap()
        );
    }
    Ok(())
}
