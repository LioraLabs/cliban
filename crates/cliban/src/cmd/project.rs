//! `cliban project` subcommands.

use std::io::Read;

use cliban_core::contexts::projects;
use cliban_core::contexts::projects::{CreateProject, UpdateProject};
use cliban_core::time::format_usec;

use crate::errors::{CliError, CliResult};
use crate::output::{build_project_json, Mode};
use crate::search::fuzzy_find;
use crate::store_open;

#[derive(clap::Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub cmd: ProjectCmd,
}

#[derive(clap::Subcommand)]
pub enum ProjectCmd {
    /// Add a project (KEY must be uppercase letters/digits, 2-10 chars)
    Add {
        /// project key (uppercase letters/digits, 2-10 chars)
        key: String,
        /// display name (default: the key)
        name: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        description: Option<String>,
        #[arg(long = "description-file")]
        description_file: Option<String>,
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// List projects
    Ls {
        #[arg(long)]
        archived: bool,
        /// include each project's `description` body (and full metadata) in --json output
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Show a project
    Show {
        /// project key (default: $CLIBAN_PROJECT)
        key: Option<String>,
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Print the raw description exactly as stored — never formatted
    Cat {
        /// project key (default: $CLIBAN_PROJECT)
        key: Option<String>,
        /// print only one section's body: spec|plan|activity|notes, or a
        /// verbatim H2 anchor
        #[arg(long)]
        section: Option<String>,
    },
    /// Fuzzy-search Markdown subsections in a project description
    Search {
        /// `[KEY] QUERY` — with a single argument it is the QUERY and the
        /// key comes from $CLIBAN_PROJECT
        #[arg(value_name = "KEY-OR-QUERY")]
        first: String,
        #[arg(value_name = "QUERY")]
        second: Option<String>,
        #[arg(long, default_value = "notes")]
        section: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
        /// human output (overrides $CLIBAN_OUTPUT and pipe detection)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Edit a project
    Edit {
        key: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        description: Option<String>,
        #[arg(long = "description-file")]
        description_file: Option<String>,
        #[arg(long = "auto-archive-done-after")]
        auto_archive_done_after: Option<String>,
        /// abort unless the project's updated_at still equals this value (as
        /// read from a prior --json); closes the read-modify-write race
        #[arg(long = "if-updated-at")]
        if_updated_at: Option<String>,
        /// JSON output (echo the updated project)
        #[arg(long)]
        json: bool,
        /// human output (one-line confirmation)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Project `## Notes` operations
    Note {
        #[command(subcommand)]
        cmd: NoteCmd,
    },
    /// Archive a project
    Archive {
        key: String,
        /// JSON output (echo the archived project)
        #[arg(long)]
        json: bool,
        /// human output (one-line confirmation)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// Unarchive a project
    Unarchive {
        key: String,
        /// JSON output (echo the unarchived project)
        #[arg(long)]
        json: bool,
        /// human output (one-line confirmation)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
    /// A unix reflex, not a real deleter (hidden): archives, says so, names
    /// the undo.
    #[command(hide = true)]
    Rm {
        key: String,
        /// accepted and ignored — archiving needs no force
        #[arg(long)]
        force: bool,
    },
}

#[derive(clap::Subcommand)]
pub enum NoteCmd {
    /// Append a `### <title>` subsection under the project's `## Notes`,
    /// leaving everything else byte-identical
    Add {
        /// `[KEY] TITLE` — with a single argument it is the note's `###`
        /// heading and the key comes from $CLIBAN_PROJECT
        #[arg(value_name = "KEY-OR-TITLE")]
        first: String,
        #[arg(value_name = "TITLE")]
        second: Option<String>,
        /// note body (use '-' for stdin)
        #[arg(long, allow_hyphen_values = true)]
        body: Option<String>,
        /// read the body from a file (use '-' for stdin)
        #[arg(long = "body-file")]
        body_file: Option<String>,
        #[arg(long)]
        json: bool,
        /// human output (one-line confirmation)
        #[arg(long, conflicts_with = "json")]
        table: bool,
    },
}

pub async fn run(db: &Option<String>, args: ProjectArgs) -> CliResult<()> {
    match args.cmd {
        ProjectCmd::Add {
            key,
            name,
            description,
            description_file,
            json,
            table,
        } => {
            add(
                db,
                key,
                name,
                description,
                description_file,
                crate::output::mode(json, table),
            )
            .await
        }
        ProjectCmd::Ls {
            archived,
            full,
            json,
            table,
        } => {
            ls(
                db,
                archived,
                crate::output::Detail::from_full_flag(full),
                crate::output::mode(json, table),
            )
            .await
        }
        ProjectCmd::Show { key, json, table } => show(db, key, json, table).await,
        ProjectCmd::Cat { key, section } => cat(db, key, section).await,
        ProjectCmd::Search {
            first,
            second,
            section,
            limit,
            json,
            table,
        } => {
            let (key, query) = match second {
                Some(q) => (crate::scope::project_identity(Some(first))?, q),
                None => (crate::scope::project_identity(None)?, first),
            };
            search(
                db,
                key,
                query,
                section,
                limit,
                crate::output::mode(json, table),
            )
            .await
        }
        ProjectCmd::Edit {
            key,
            name,
            description,
            description_file,
            auto_archive_done_after,
            if_updated_at,
            json,
            table,
        } => {
            edit(
                db,
                key,
                name,
                description,
                description_file,
                auto_archive_done_after,
                if_updated_at,
                crate::output::mode(json, table),
            )
            .await
        }
        ProjectCmd::Note { cmd } => match cmd {
            NoteCmd::Add {
                first,
                second,
                body,
                body_file,
                json,
                table,
            } => {
                let (key, title) = match second {
                    Some(t) => (crate::scope::project_identity(Some(first))?, t),
                    None => (crate::scope::project_identity(None)?, first),
                };
                note_add(
                    db,
                    key,
                    title,
                    body,
                    body_file,
                    crate::output::mode(json, table),
                )
                .await
            }
        },
        ProjectCmd::Archive { key, json, table } => {
            let p = set_archived(db, key, true).await?;
            confirm_project(&p, "archived project", crate::output::mode(json, table))
        }
        ProjectCmd::Unarchive { key, json, table } => {
            let p = set_archived(db, key, false).await?;
            confirm_project(&p, "unarchived project", crate::output::mode(json, table))
        }
        ProjectCmd::Rm { key, force: _ } => {
            // Do the closest safe thing rather than spending a turn refusing.
            // The message IS the point — it prints in every mode.
            let key = key.to_uppercase();
            set_archived(db, key.clone(), true).await?;
            println!(
                "archived project {key} — cliban archives instead of deleting \
                 (undo: cliban project unarchive {key})"
            );
            Ok(())
        }
    }
}

/// The mutation contract's success report for a project: one line in table
/// mode, the `show --json` shape in json mode. Never silent.
fn confirm_project(p: &cliban_core::schema::Project, verb: &str, mode: Mode) -> CliResult<()> {
    if mode.is_json() {
        println!(
            "{}",
            serde_json::to_string(&project_json_detail(p, crate::output::Detail::Echo)).unwrap()
        );
    } else {
        println!("{verb} {}", p.key);
    }
    Ok(())
}

fn project_json(p: &cliban_core::schema::Project) -> serde_json::Value {
    project_json_detail(p, crate::output::Detail::Full)
}

fn project_json_detail(
    p: &cliban_core::schema::Project,
    detail: crate::output::Detail,
) -> serde_json::Value {
    build_project_json(
        &p.key,
        &p.name,
        &p.description,
        p.archived,
        p.auto_archive_done_after_days,
        p.issue_seq,
        &format_usec(p.inserted_at),
        &format_usec(p.updated_at),
        detail,
    )
}

async fn add(
    db: &Option<String>,
    key: String,
    name: Option<String>,
    description: Option<String>,
    description_file: Option<String>,
    mode: Mode,
) -> CliResult<()> {
    let key = key.to_uppercase();
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| key.clone());
    let description = resolve_description(description, description_file)?;
    let store = store_open::open(db).await?;
    let p = store
        .call(move |conn| {
            projects::create(
                conn,
                CreateProject {
                    key,
                    name,
                    description,
                    auto_archive_done_after_days: None,
                },
            )
        })
        .await?;
    if mode.is_json() {
        let v = project_json_detail(&p, crate::output::Detail::Echo);
        println!("{}", serde_json::to_string(&v).unwrap());
    } else {
        println!("created project {} ({})", p.key, p.name);
    }
    Ok(())
}

async fn ls(
    db: &Option<String>,
    archived: bool,
    detail: crate::output::Detail,
    mode: Mode,
) -> CliResult<()> {
    let store = store_open::open(db).await?;
    let mut ps = store.call(projects::list).await?;
    if !archived {
        ps.retain(|p| !p.archived);
    }
    if mode.is_json() {
        for p in &ps {
            let v = project_json_detail(p, detail);
            println!("{}", serde_json::to_string(&v).unwrap());
        }
    } else {
        for p in &ps {
            let mark = if p.archived { " (archived)" } else { "" };
            println!("{:<10} {}{}", p.key, p.name, mark);
        }
    }
    Ok(())
}

async fn show(db: &Option<String>, key: Option<String>, json: bool, table: bool) -> CliResult<()> {
    let key = crate::scope::project_identity(key)?;
    let store = store_open::open(db).await?;
    let p = store
        .call(move |conn| projects::fetch_by_key(conn, &key))
        .await?;
    if crate::output::mode(json, table).is_json() {
        let v = project_json(&p);
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        println!("{} — {}\n{}", p.key, p.name, p.description);
    }
    Ok(())
}

/// `project cat`: the same bytes-only contract as `issue cat` — the whole
/// description, or one section's body, verbatim in every mode.
async fn cat(db: &Option<String>, key: Option<String>, section: Option<String>) -> CliResult<()> {
    let key = crate::scope::project_identity(key)?;
    let store = store_open::open(db).await?;
    let p = store
        .call(move |conn| projects::fetch_by_key(conn, &key))
        .await?;
    match section {
        Some(section) => {
            let body = description_section(&p.description, &section)?;
            print!("{body}");
        }
        None => print!("{}", p.description),
    }
    Ok(())
}

#[derive(Debug)]
struct MarkdownHit {
    heading: String,
    content: String,
    score: i64,
}

async fn search(
    db: &Option<String>,
    key: String,
    query: String,
    section: String,
    limit: usize,
    mode: Mode,
) -> CliResult<()> {
    let terms = query.split_whitespace().collect::<Vec<_>>();
    if terms.is_empty() {
        return Err(CliError::validation("search query can't be blank"));
    }
    if limit == 0 {
        return Err(CliError::validation("--limit must be at least 1"));
    }
    let key = key.to_uppercase();
    let project_key = key.clone();
    let store = store_open::open(db).await?;
    let project = store
        .call(move |conn| projects::fetch_by_key(conn, &key))
        .await?;
    let markdown = description_section(&project.description, &section)?;
    let mut hits = markdown_subsections(markdown)
        .into_iter()
        .filter_map(|(heading, content)| {
            let searchable = format!("{heading}\n{content}");
            let score = terms.iter().try_fold(0_i64, |score, term| {
                fuzzy_find(term, &searchable).map(|(term_score, _)| score + term_score)
            })?;
            Some(MarkdownHit {
                heading,
                content,
                score,
            })
        })
        .collect::<Vec<_>>();
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.heading.cmp(&b.heading))
    });
    hits.truncate(limit);

    for hit in hits {
        if mode.is_json() {
            println!(
                "{}",
                serde_json::json!({
                    "content": hit.content,
                    "heading": hit.heading,
                    "project": project_key,
                    "score": hit.score,
                })
            );
        } else {
            print!("{}", hit.content);
            if !hit.content.ends_with('\n') {
                println!();
            }
        }
    }
    Ok(())
}

fn description_section<'a>(description: &'a str, section: &str) -> CliResult<&'a str> {
    match section.to_ascii_lowercase().as_str() {
        "all" => Ok(description),
        "notes" => markdown_section(description, "Notes")
            .ok_or_else(|| CliError::not_found("not found: no ## Notes section")),
        _ => Err(CliError::validation("invalid --section (want notes|all)")),
    }
}

fn markdown_section<'a>(markdown: &'a str, anchor: &str) -> Option<&'a str> {
    let needle = format!("## {anchor}");
    let mut offset = 0;
    let mut content_start = None;
    let mut fence = None;
    for line in markdown.split_inclusive('\n') {
        if let Some((fence_char, fence_len)) = fence {
            if is_closing_fence(line, fence_char, fence_len) {
                fence = None;
            }
            offset += line.len();
            continue;
        }
        if let Some(marker) = fence_marker(line) {
            fence = Some(marker);
            offset += line.len();
            continue;
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Some(start) = content_start {
            if trimmed.starts_with("## ") {
                return Some(&markdown[start..offset]);
            }
        } else if trimmed == needle {
            content_start = Some(offset + line.len());
        }
        offset += line.len();
    }
    content_start.map(|start| &markdown[start..])
}

fn markdown_subsections(markdown: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut heading = None;
    let mut content = String::new();
    let mut fence = None;
    for line in markdown.split_inclusive('\n') {
        if let Some((fence_char, fence_len)) = fence {
            if heading.is_some() {
                content.push_str(line);
            }
            if is_closing_fence(line, fence_char, fence_len) {
                fence = None;
            }
            continue;
        }
        if let Some(marker) = fence_marker(line) {
            if heading.is_some() {
                content.push_str(line);
            }
            fence = Some(marker);
            continue;
        }
        if let Some(title) = line.strip_prefix("### ") {
            if let Some(previous_heading) = heading.replace(title.trim().to_string()) {
                sections.push((previous_heading, content));
            }
            content = line.to_string();
        } else if line.starts_with("# ") || line.starts_with("## ") {
            if let Some(previous_heading) = heading.take() {
                sections.push((previous_heading, content));
                content = String::new();
            }
        } else if heading.is_some() {
            content.push_str(line);
        }
    }
    if let Some(heading) = heading {
        sections.push((heading, content));
    }
    sections
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let count = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (count >= 3).then_some((marker, count))
}

fn is_closing_fence(line: &str, fence_char: char, fence_len: usize) -> bool {
    let Some((marker, count)) = fence_marker(line) else {
        return false;
    };
    if marker != fence_char || count < fence_len {
        return false;
    }
    let trimmed = line.trim_start_matches(' ');
    trimmed[count..]
        .trim_matches([' ', '\t', '\r', '\n'])
        .is_empty()
}

fn resolve_description(
    description: Option<String>,
    description_file: Option<String>,
) -> CliResult<Option<String>> {
    if description.is_some() && description_file.is_some() {
        return Err(CliError::validation(
            "--description and --description-file are mutually exclusive",
        ));
    }
    match (description, description_file) {
        // A bare `-` is the stdin sentinel on the value flag too, not a
        // one-character payload — same as `issue` and `milestone`. A
        // hyphen-leading value like `- a bullet` is ordinary text.
        (Some(value), None) if value == "-" => Ok(Some(read_stdin()?)),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(path)) if path == "-" => Ok(Some(read_stdin()?)),
        (None, Some(path)) => std::fs::read_to_string(path)
            .map(Some)
            .map_err(|error| CliError::validation(error.to_string())),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => unreachable!(),
    }
}

fn read_stdin() -> CliResult<String> {
    let mut description = String::new();
    std::io::stdin()
        .read_to_string(&mut description)
        .map_err(|error| CliError::validation(error.to_string()))?;
    Ok(description)
}

/// Parses a simple `Nd` / `N` (days) string. `""`/`"0"` mean "disabled" (0).
fn parse_duration_days(s: &str) -> CliResult<i64> {
    let s = s.trim();
    if s.is_empty() || s == "0" {
        return Ok(0);
    }
    let trimmed = s.strip_suffix('d').unwrap_or(s);
    match trimmed.parse::<i64>() {
        Ok(n) if n >= 0 => Ok(n),
        _ => Err(CliError::validation(format!(
            "invalid duration {s:?} (use e.g. 7d or 0 to disable)"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
async fn edit(
    db: &Option<String>,
    key: String,
    name: Option<String>,
    description: Option<String>,
    description_file: Option<String>,
    auto_archive_done_after: Option<String>,
    if_updated_at: Option<String>,
    mode: Mode,
) -> CliResult<()> {
    let key = key.to_uppercase();
    let description = resolve_description(description, description_file)?;
    // Parse the duration before opening the store so a bad value fails fast
    // and nothing is half-written: name/description first, then the
    // auto-archive duration, both inside one store call.
    let days = match &auto_archive_done_after {
        Some(s) => Some(parse_duration_days(s)?),
        None => None,
    };
    let store = store_open::open(db).await?;
    let p = store
        .call(move |conn| {
            let cur = projects::fetch_by_key(conn, &key)?;
            if let Some(expected) = if_updated_at.as_deref() {
                let actual = format_usec(cur.updated_at);
                if actual != expected {
                    return Err(cliban_core::Error::validation(
                        "updated_at",
                        &format!(
                            "stale write: {} was updated at {actual}, you read {expected} — \
                             re-read and retry",
                            cur.key
                        ),
                    ));
                }
            }
            let new_name = name.unwrap_or_else(|| cur.name.clone());
            let new_desc = description.unwrap_or_else(|| cur.description.clone());
            projects::update(
                conn,
                &cur,
                UpdateProject {
                    name: Some(new_name),
                    description: Some(new_desc),
                    archived: None,
                    auto_archive_done_after_days: None,
                },
            )?;
            if let Some(days) = days {
                let cur = projects::fetch_by_key(conn, &key)?;
                projects::update(
                    conn,
                    &cur,
                    UpdateProject {
                        auto_archive_done_after_days: Some(Some(days)),
                        ..Default::default()
                    },
                )?;
            }
            projects::fetch_by_key(conn, &key)
        })
        .await?;
    confirm_project(&p, "updated project", mode)
}

/// Flip a project's archived bit and return the updated row. Prints nothing:
/// `archive`/`unarchive` confirm via [`confirm_project`], while `rm` keeps
/// its own always-on message.
async fn set_archived(
    db: &Option<String>,
    key: String,
    archived: bool,
) -> CliResult<cliban_core::schema::Project> {
    let key = key.to_uppercase();
    let store = store_open::open(db).await?;
    let p = store
        .call(move |conn| {
            let cur = projects::fetch_by_key(conn, &key)?;
            projects::update(
                conn,
                &cur,
                UpdateProject {
                    archived: Some(archived),
                    ..Default::default()
                },
            )?;
            projects::fetch_by_key(conn, &key)
        })
        .await?;
    Ok(p)
}

/// `project note add`: append one `### <title>` subsection under `## Notes`,
/// splicing via the section machinery so the rest of the description — and
/// every earlier note — stays byte-identical. This is the safe path the
/// promote-lesson flow needs: no whole-description rewrite.
async fn note_add(
    db: &Option<String>,
    key: String,
    title: String,
    body: Option<String>,
    body_file: Option<String>,
    mode: Mode,
) -> CliResult<()> {
    let key = key.to_uppercase();
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err(CliError::validation("the note title can't be blank"));
    }
    let body = match resolve_description(body, body_file)? {
        Some(explicit) => explicit,
        // No --body, no --body-file: piped/redirected stdin is the body. An
        // empty pipe (or a TTY) means "no body" — bare heading — because the
        // body is optional here, unlike log/append-section.
        None => crate::stdin_input::fallback()?.unwrap_or_default(),
    };
    let sub = if body.trim().is_empty() {
        format!("### {title}")
    } else {
        format!("### {title}\n\n{}", body.trim_end())
    };
    let store = store_open::open(db).await?;
    let k = key.clone();
    store
        .call(move |conn| {
            let cur = projects::fetch_by_key(conn, &k)?;
            let (start, end, found) =
                cliban_core::sections::find_section(&cur.description, "Notes");
            let new_body = if found {
                let existing = cur.description[start..end].trim_matches('\n');
                if existing.is_empty() {
                    sub
                } else {
                    format!("{existing}\n\n{sub}")
                }
            } else {
                sub
            };
            let new_desc =
                cliban_core::sections::replace_section(&cur.description, "Notes", &new_body);
            projects::update(
                conn,
                &cur,
                UpdateProject {
                    description: Some(new_desc),
                    ..Default::default()
                },
            )?;
            Ok(())
        })
        .await?;
    if mode.is_json() {
        println!(
            "{}",
            serde_json::json!({ "key": key, "note": title, "section": "Notes" })
        );
    } else {
        println!("added note {title:?} to {key} ## Notes");
    }
    Ok(())
}
