//! `cliban project` subcommands. Output is byte-for-byte parity with the Go
//! oracle (`internal/cli/project.go`).

use std::io::Read;

use cliban_core::contexts::issues::ListOpts;
use cliban_core::contexts::projects::{CreateProject, UpdateProject};
use cliban_core::contexts::{issues, projects};
use cliban_core::time::format_usec;

use crate::errors::{CliError, CliResult};
use crate::output::build_project_json;
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
        key: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "description-file")]
        description_file: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List projects
    Ls {
        #[arg(long)]
        archived: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show a project
    Show {
        key: String,
        #[arg(long)]
        section: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Fuzzy-search Markdown subsections in a project description
    Search {
        key: String,
        query: String,
        #[arg(long, default_value = "notes")]
        section: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Edit a project
    Edit {
        key: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "description-file")]
        description_file: Option<String>,
        #[arg(long = "auto-archive-done-after")]
        auto_archive_done_after: Option<String>,
    },
    /// Archive a project
    Archive { key: String },
    /// Unarchive a project
    Unarchive { key: String },
    /// Delete a project
    Rm {
        key: String,
        #[arg(long)]
        force: bool,
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
        } => add(db, key, name, description, description_file, json).await,
        ProjectCmd::Ls { archived, json } => ls(db, archived, json).await,
        ProjectCmd::Show { key, section, json } => show(db, key, section, json).await,
        ProjectCmd::Search {
            key,
            query,
            section,
            limit,
            json,
        } => search(db, key, query, section, limit, json).await,
        ProjectCmd::Edit {
            key,
            name,
            description,
            description_file,
            auto_archive_done_after,
        } => {
            edit(
                db,
                key,
                name,
                description,
                description_file,
                auto_archive_done_after,
            )
            .await
        }
        ProjectCmd::Archive { key } => set_archived(db, key, true).await,
        ProjectCmd::Unarchive { key } => set_archived(db, key, false).await,
        ProjectCmd::Rm { key, force } => rm(db, key, force).await,
    }
}

fn project_json(p: &cliban_core::schema::Project) -> serde_json::Value {
    build_project_json(
        &p.key,
        &p.name,
        &p.description,
        p.archived,
        p.auto_archive_done_after_days,
        p.issue_seq,
        &format_usec(p.inserted_at),
        &format_usec(p.updated_at),
    )
}

async fn add(
    db: &Option<String>,
    key: String,
    name: String,
    description: Option<String>,
    description_file: Option<String>,
    json: bool,
) -> CliResult<()> {
    let key = key.to_uppercase();
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
    if json {
        let v = project_json(&p);
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        println!("created project {} ({})", p.key, p.name);
    }
    Ok(())
}

async fn ls(db: &Option<String>, archived: bool, json: bool) -> CliResult<()> {
    let store = store_open::open(db).await?;
    let mut ps = store.call(projects::list).await?;
    if !archived {
        ps.retain(|p| !p.archived);
    }
    if json {
        for p in &ps {
            let v = project_json(p);
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

async fn show(
    db: &Option<String>,
    key: String,
    section: Option<String>,
    json: bool,
) -> CliResult<()> {
    let key = key.to_uppercase();
    let store = store_open::open(db).await?;
    let p = store
        .call(move |conn| projects::fetch_by_key(conn, &key))
        .await?;
    if let Some(section) = section {
        if json {
            return Err(CliError::validation(
                "--section and --json are mutually exclusive",
            ));
        }
        let body = description_section(&p.description, &section)?;
        print!("{body}");
        return Ok(());
    }
    if json {
        let v = project_json(&p);
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        println!("{} — {}\n{}", p.key, p.name, p.description);
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
    json: bool,
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
        if json {
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

async fn edit(
    db: &Option<String>,
    key: String,
    name: Option<String>,
    description: Option<String>,
    description_file: Option<String>,
    auto_archive_done_after: Option<String>,
) -> CliResult<()> {
    let key = key.to_uppercase();
    let description = resolve_description(description, description_file)?;
    // Parse the duration before opening the store so a bad value fails fast,
    // matching Go's order of effects only for the auto-archive update (which
    // Go runs after the name/description update — but parse errors there abort
    // before any DB write of the duration). Go updates name/desc first, then
    // sets the duration. We mirror that: parse here, write both via store.
    let days = match &auto_archive_done_after {
        Some(s) => Some(parse_duration_days(s)?),
        None => None,
    };
    let store = store_open::open(db).await?;
    store
        .call(move |conn| {
            let cur = projects::fetch_by_key(conn, &key)?;
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
            Ok(())
        })
        .await?;
    Ok(())
}

async fn set_archived(db: &Option<String>, key: String, archived: bool) -> CliResult<()> {
    let key = key.to_uppercase();
    let store = store_open::open(db).await?;
    store
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
            Ok(())
        })
        .await?;
    Ok(())
}

async fn rm(db: &Option<String>, key: String, force: bool) -> CliResult<()> {
    let key = key.to_uppercase();
    let store = store_open::open(db).await?;
    store
        .call(move |conn| {
            let issues = issues::list(
                conn,
                ListOpts {
                    project: Some(&key),
                    archived: false,
                    ..Default::default()
                },
            )?;
            if !issues.is_empty() && !force {
                return Err(cliban_core::Error::validation(
                    "project",
                    &format!(
                        "project {} has {} issues; pass --force to delete",
                        key,
                        issues.len()
                    ),
                ));
            }
            let cur = projects::fetch_by_key(conn, &key)?;
            projects::delete(conn, &cur)
        })
        .await?;
    Ok(())
}
