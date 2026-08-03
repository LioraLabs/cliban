//! Port of `backend/lib/loom/milestones.ex` + `Loom.Schema.Milestone`.

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::contexts::projects;
use crate::error::{Error, Result};
use crate::rows;
use crate::schema::{Milestone, DONE_STATUS, MILESTONE_STATUSES};
use crate::time;

#[derive(Debug, Clone)]
pub struct CreateMilestone {
    /// Project key (resolved to project_id). Mirrors the `%{project: key}`
    /// create clause.
    pub project: String,
    pub name: String,
    pub description: Option<String>,
    pub target_date: Option<NaiveDate>,
    pub status: Option<String>,
}

pub fn create(conn: &Connection, attrs: CreateMilestone) -> Result<Milestone> {
    let project = match projects::get_by_key(conn, &attrs.project)? {
        Some(p) => p,
        None => {
            return Err(Error::validation(
                "project",
                &format!("project not found: {}", attrs.project),
            ))
        }
    };

    if attrs.name.is_empty() {
        return Err(Error::validation("name", "can't be blank"));
    }
    let status = attrs.status.clone().unwrap_or_else(|| "open".to_string());
    if !MILESTONE_STATUSES.contains(&status.as_str()) {
        return Err(Error::validation("status", "is invalid"));
    }
    // unique_constraint([:project_id, :name])
    if get_row(conn, project.id, &attrs.name)?.is_some() {
        return Err(Error::validation("name", "already exists in this project"));
    }

    let now = time::format_usec(time::now_usec());
    let desc = attrs.description.unwrap_or_default();
    let target = attrs.target_date.map(time::format_date);
    conn.execute(
        "INSERT INTO milestones (project_id, name, description, target_date, \
         status, archived, inserted_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
        params![project.id, attrs.name, desc, target, status, now],
    )?;
    let id = conn.last_insert_rowid();
    Ok(get_by_id(conn, id)?.expect("inserted"))
}

#[derive(Debug, Default, Clone)]
pub struct UpdateMilestone {
    pub name: Option<String>,
    pub description: Option<String>,
    pub target_date: Option<Option<NaiveDate>>,
    pub status: Option<String>,
    pub archived: Option<bool>,
}

pub fn update(conn: &Connection, m: &Milestone, attrs: UpdateMilestone) -> Result<Milestone> {
    if let Some(name) = &attrs.name {
        if name.is_empty() {
            return Err(Error::validation("name", "can't be blank"));
        }
    }
    if let Some(status) = &attrs.status {
        if !MILESTONE_STATUSES.contains(&status.as_str()) {
            return Err(Error::validation("status", "is invalid"));
        }
    }
    let name = attrs.name.clone().unwrap_or_else(|| m.name.clone());
    // unique within project, if name changed
    if name != m.name {
        if let Some(existing) = get_row(conn, m.project_id, &name)? {
            if existing.id != m.id {
                return Err(Error::validation("name", "already exists in this project"));
            }
        }
    }
    let description = attrs
        .description
        .clone()
        .unwrap_or_else(|| m.description.clone());
    let target = match attrs.target_date {
        Some(v) => v,
        None => m.target_date,
    };
    let status = attrs.status.clone().unwrap_or_else(|| m.status.clone());
    let archived = attrs.archived.unwrap_or(m.archived);
    let now = time::format_usec(time::now_usec());

    conn.execute(
        "UPDATE milestones SET name = ?1, description = ?2, target_date = ?3, \
         status = ?4, archived = ?5, updated_at = ?6 WHERE id = ?7",
        params![
            name,
            description,
            target.map(time::format_date),
            status,
            archived as i64,
            now,
            m.id
        ],
    )?;
    Ok(get_by_id(conn, m.id)?.expect("updated"))
}

/// `list/1` — ordered by name; optionally scoped to a project key. An unknown
/// project key yields an empty list (mirrors `where: false`).
pub fn list(conn: &Connection, project: Option<&str>) -> Result<Vec<Milestone>> {
    let (where_clause, project_id) = match project {
        None => (String::new(), None),
        Some(key) => match projects::get_by_key(conn, key)? {
            Some(p) => ("WHERE project_id = ?1".to_string(), Some(p.id)),
            None => return Ok(vec![]),
        },
    };
    let sql = format!(
        "SELECT {} FROM milestones {} ORDER BY name ASC",
        rows::MILESTONE_COLS,
        where_clause
    );
    let mut stmt = conn.prepare(&sql)?;
    let out = match project_id {
        Some(pid) => stmt
            .query_map(params![pid], rows::milestone)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        None => stmt
            .query_map([], rows::milestone)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    };
    Ok(out)
}

/// A milestone plus the rollups a milestone-centric view needs: the owning
/// project's key, non-archived issue counts, and when the milestone was last
/// *worked on* (as opposed to last edited).
#[derive(Debug, Clone)]
pub struct MilestoneSummary {
    pub milestone: Milestone,
    pub project_key: String,
    /// Non-archived issues carrying this milestone.
    pub total: i64,
    /// …of which are `done`.
    pub done: i64,
    /// When the milestone was last *worked on*: the newest of any activity-log
    /// entry on its issues, any issue's `updated_at`, and the milestone row's
    /// own `updated_at`. Issue `updated_at` carries the weight in practice —
    /// every move/edit bumps it, while `activity_log_entries` is only written
    /// by callers that use the log API directly.
    pub last_activity: DateTime<Utc>,
}

impl MilestoneSummary {
    /// Completion as a 0.0–1.0 fraction; an empty milestone reads as 0.
    pub fn progress(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.done as f64 / self.total as f64
        }
    }
}

/// Orderings for [`summaries`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// Most recently worked on first — the milestone page's default.
    #[default]
    Activity,
    Name,
    /// Soonest target first; undated milestones sort last.
    Target,
}

impl Sort {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "activity" => Some(Self::Activity),
            "name" => Some(Self::Name),
            "target" => Some(Self::Target),
            _ => None,
        }
    }

    fn order_by(self) -> &'static str {
        match self {
            // `last_activity` is the SELECT alias below.
            Self::Activity => "last_activity DESC, m.name ASC",
            Self::Name => "p.key ASC, m.name ASC",
            Self::Target => "m.target_date IS NULL, m.target_date ASC, m.name ASC",
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SummaryOpts<'a> {
    /// Project key; `None` spans every project.
    pub project: Option<&'a str>,
    /// Exact `status` filter; `None` keeps all statuses.
    pub status: Option<&'a str>,
    pub sort: Sort,
}

/// `list/1` with rollups. An unknown project key yields an empty list, same as
/// [`list`].
pub fn summaries(conn: &Connection, opts: SummaryOpts) -> Result<Vec<MilestoneSummary>> {
    let project_id = match opts.project {
        None => None,
        Some(key) => match projects::get_by_key(conn, key)? {
            Some(p) => Some(p.id),
            None => return Ok(vec![]),
        },
    };

    // The counts are correlated subqueries rather than joins: joining both
    // `issues` and `activity_log_entries` would multiply rows and inflate the
    // counts. Both filters are always bound and no-op on NULL so the statement
    // shape (and its param set) stays constant.
    let sql = format!(
        "SELECT {cols}, p.key, \
           (SELECT COUNT(*) FROM issues i \
              WHERE i.milestone_id = m.id AND i.archived = 0) AS total, \
           (SELECT COUNT(*) FROM issues i \
              WHERE i.milestone_id = m.id AND i.archived = 0 AND i.status = '{done}') AS done_count, \
           MAX(COALESCE((SELECT MAX(a.ts) FROM activity_log_entries a \
                 JOIN issues i ON i.id = a.issue_id \
                 WHERE i.milestone_id = m.id AND i.archived = 0), ''), \
               COALESCE((SELECT MAX(i.updated_at) FROM issues i \
                 WHERE i.milestone_id = m.id AND i.archived = 0), ''), \
               m.updated_at) AS last_activity \
         FROM milestones m JOIN projects p ON p.id = m.project_id \
         WHERE (?1 IS NULL OR m.project_id = ?1) AND (?2 IS NULL OR m.status = ?2) \
         ORDER BY {order}",
        cols = rows::milestone_cols_as("m"),
        done = DONE_STATUS,
        order = opts.sort.order_by(),
    );

    let mut stmt = conn.prepare(&sql)?;
    let out = stmt
        .query_map(params![project_id, opts.status], |row| {
            let milestone = rows::milestone(row)?;
            let last: String = row.get(12)?;
            Ok(MilestoneSummary {
                last_activity: time::parse_ts(&last).unwrap_or(milestone.updated_at),
                milestone,
                project_key: row.get(9)?,
                total: row.get(10)?,
                done: row.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

/// `get/2` — by project key + name.
pub fn get(conn: &Connection, project_key: &str, name: &str) -> Result<Option<Milestone>> {
    match projects::get_by_key(conn, project_key)? {
        None => Ok(None),
        Some(p) => get_row(conn, p.id, name),
    }
}

pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<Milestone>> {
    let sql = format!(
        "SELECT {} FROM milestones WHERE id = ?1",
        rows::MILESTONE_COLS
    );
    Ok(conn
        .query_row(&sql, params![id], rows::milestone)
        .optional()?)
}

fn get_row(conn: &Connection, project_id: i64, name: &str) -> Result<Option<Milestone>> {
    let sql = format!(
        "SELECT {} FROM milestones WHERE project_id = ?1 AND name = ?2",
        rows::MILESTONE_COLS
    );
    Ok(conn
        .query_row(&sql, params![project_id, name], rows::milestone)
        .optional()?)
}

// ---- waves ----

/// The dependency-wave partition of a milestone, for orchestration: wave N is
/// safe to start once waves 0..N have finished. Derived from the `blocks`
/// edges among the milestone's own open issues; done issues count as satisfied
/// dependencies and are listed separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waves {
    /// Layers of issue keys: everything in `waves[n]` has all its blockers in
    /// `done` or in earlier waves.
    pub waves: Vec<Vec<String>>,
    /// The milestone's issues already done (their dependents may schedule).
    pub done: Vec<String>,
    /// Open issues gated — directly or transitively — by open work outside
    /// this milestone. They cannot be scheduled by finishing the milestone's
    /// own waves.
    pub external_blocked: Vec<String>,
}

/// Partition the milestone's open, non-archived issues into dependency waves
/// (Kahn layers over the intra-milestone `blocks` edges). A dependency cycle is
/// an error naming the issues involved — a cyclic milestone cannot be
/// orchestrated and the board needs fixing, not a guess.
pub fn waves(conn: &Connection, project_key: &str, name: &str) -> Result<Waves> {
    use std::collections::{HashMap, HashSet};

    let milestone = get(conn, project_key, name)?.ok_or(Error::NotFound)?;

    struct Node {
        key: String,
        done: bool,
    }
    let sql = "SELECT id, key, status FROM issues \
               WHERE milestone_id = ?1 AND archived = 0 ORDER BY position, key";
    let mut stmt = conn.prepare(sql)?;
    let nodes: HashMap<i64, Node> = stmt
        .query_map(params![milestone.id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Node {
                    key: r.get(1)?,
                    done: r.get::<_, String>(2)? == crate::schema::DONE_STATUS,
                },
            ))
        })?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;

    let done: Vec<String> = nodes
        .values()
        .filter(|n| n.done)
        .map(|n| n.key.clone())
        .collect();

    // Open blockers of every open node, split into edges inside the milestone
    // and gates from outside it.
    let mut intra: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut external: HashSet<i64> = HashSet::new();
    {
        let mut stmt = conn.prepare(
            "SELECT r.to_issue_id, b.id FROM issue_relation r \
             JOIN issues b ON b.id = r.from_issue_id \
             JOIN issues i ON i.id = r.to_issue_id \
             WHERE r.type = 'blocks' AND i.milestone_id = ?1 AND i.archived = 0 \
             AND b.archived = 0 AND b.status != 'done'",
        )?;
        let edges = stmt
            .query_map(params![milestone.id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (to, blocker) in edges {
            if nodes.get(&to).is_none_or(|n| n.done) {
                continue;
            }
            if nodes.contains_key(&blocker) {
                intra.entry(to).or_default().push(blocker);
            } else {
                external.insert(to);
            }
        }
    }

    // Layered Kahn over the open nodes. External-gated nodes never schedule;
    // whatever else is left at the fixpoint is either transitively external
    // or a genuine cycle.
    let mut remaining: HashSet<i64> = nodes
        .iter()
        .filter(|(_, n)| !n.done)
        .map(|(id, _)| *id)
        .collect();
    let mut scheduled: HashSet<i64> = HashSet::new();
    let mut waves_out: Vec<Vec<String>> = Vec::new();
    loop {
        let mut wave: Vec<i64> = remaining
            .iter()
            .copied()
            .filter(|id| !external.contains(id))
            .filter(|id| {
                intra
                    .get(id)
                    .is_none_or(|blockers| blockers.iter().all(|b| scheduled.contains(b)))
            })
            .collect();
        if wave.is_empty() {
            break;
        }
        wave.sort_by(|a, b| nodes[a].key.cmp(&nodes[b].key));
        for id in &wave {
            remaining.remove(id);
            scheduled.insert(*id);
        }
        waves_out.push(wave.into_iter().map(|id| nodes[&id].key.clone()).collect());
    }

    // Drain the transitively-external tail: nodes whose unmet blockers are all
    // external-blocked themselves. What survives is a cycle.
    let mut external_blocked: HashSet<i64> = external
        .iter()
        .copied()
        .filter(|id| remaining.contains(id))
        .collect();
    loop {
        let grown: Vec<i64> = remaining
            .iter()
            .copied()
            .filter(|id| !external_blocked.contains(id))
            .filter(|id| {
                intra.get(id).is_none_or(|blockers| {
                    blockers
                        .iter()
                        .all(|b| scheduled.contains(b) || external_blocked.contains(b))
                })
            })
            .collect();
        if grown.is_empty() {
            break;
        }
        external_blocked.extend(grown);
    }
    let cyclic: Vec<&i64> = remaining
        .iter()
        .filter(|id| !external_blocked.contains(id))
        .collect();
    if !cyclic.is_empty() {
        let mut keys: Vec<String> = cyclic.iter().map(|id| nodes[id].key.clone()).collect();
        keys.sort();
        return Err(Error::validation(
            "milestone",
            &format!("dependency cycle among: {}", keys.join(", ")),
        ));
    }

    let mut external_keys: Vec<String> = external_blocked
        .iter()
        .map(|id| nodes[id].key.clone())
        .collect();
    external_keys.sort();
    let mut done_sorted = done;
    done_sorted.sort();

    Ok(Waves {
        waves: waves_out,
        done: done_sorted,
        external_blocked: external_keys,
    })
}
