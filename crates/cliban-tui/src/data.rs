//! Synchronous bridge from the TUI to the async `cliban_core::Store`.
use std::path::Path;

use cliban_core::contexts::{issues, milestones, projects};
use cliban_core::Store;

use crate::app::{Card, MilestoneRef};
use crate::buffers::{IssueBuffer, MilestoneBuffer, ProjectBuffer};

pub struct Data {
    pub(crate) store: Store,
    pub(crate) rt: tokio::runtime::Runtime,
    /// Called after every successful mutation (i.e. after commit — each
    /// mutation is one `store.call`). Hosts hook this to publish coarse
    /// change events; `None` (the local TUI) publishes nowhere.
    on_mutate: Option<Box<dyn Fn() + Send + Sync>>,
}

#[derive(Debug)]
pub struct DataError(pub String);
impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for DataError {}
impl From<cliban_core::Error> for DataError {
    fn from(e: cliban_core::Error) -> Self {
        DataError(e.to_string())
    }
}

impl Data {
    pub fn open(path: &Path) -> Result<Self, DataError> {
        Self::from_store(Store::open(path)?)
    }

    /// Wrap an already-open store (e.g. a shared per-tenant store handed to
    /// an SSH session). Each `Data` gets its own private blocking runtime;
    /// the store is `Clone` and serializes all callers on one writer thread.
    pub fn from_store(store: Store) -> Result<Self, DataError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| DataError(e.to_string()))?;
        Ok(Self {
            store,
            rt,
            on_mutate: None,
        })
    }

    /// Register a hook invoked after every successful mutation.
    pub fn set_on_mutate(&mut self, f: impl Fn() + Send + Sync + 'static) {
        self.on_mutate = Some(Box::new(f));
    }

    fn notify(&self) {
        if let Some(f) = &self.on_mutate {
            f();
        }
    }

    pub fn load_cards(&self) -> Result<Vec<Card>, DataError> {
        let rows = self.rt.block_on(self.store.call(|conn| {
            let list = issues::list(conn, issues::ListOpts::default())?;
            let mut out = Vec::with_capacity(list.len());
            for i in &list {
                let project = projects::get_by_id(conn, i.project_id)?
                    .map(|p| p.key)
                    .unwrap_or_default();
                let milestone = match i.milestone_id {
                    Some(m) => milestones::get_by_id(conn, m)?.map(|x| x.name),
                    None => None,
                };
                out.push((i.clone(), project, milestone));
            }
            Ok(out)
        }))?;
        Ok(rows
            .into_iter()
            .map(|(i, project, milestone)| Card {
                id: i.id,
                key: i.key,
                project,
                title: i.title,
                status: i.status,
                priority: i.priority,
                position: i.position,
                milestone_id: i.milestone_id,
                milestone,
            })
            .collect())
    }

    pub fn load_milestones(&self, project: Option<&str>) -> Result<Vec<MilestoneRef>, DataError> {
        let Some(project) = project else {
            return Ok(vec![]);
        };
        let project = project.to_string();
        let ms = self.rt.block_on(
            self.store
                .call(move |conn| milestones::list(conn, Some(&project))),
        )?;
        Ok(ms
            .into_iter()
            .map(|m| MilestoneRef {
                id: m.id,
                name: m.name,
                status: m.status,
                target: m.target_date.map(|d| d.format("%Y-%m-%d").to_string()),
            })
            .collect())
    }

    pub fn list_projects(&self) -> Result<Vec<(String, String)>, DataError> {
        let ps = self.rt.block_on(self.store.call(projects::list))?;
        Ok(ps.into_iter().map(|p| (p.key, p.name)).collect())
    }

    pub fn move_issue(&self, key: &str, status: &str) -> Result<(), DataError> {
        let (key, status) = (key.to_string(), status.to_string());
        self.rt.block_on(self.store.call(move |conn| {
            let i = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            issues::move_issue(conn, &i, &status)?;
            Ok(())
        }))?;
        self.notify();
        Ok(())
    }

    /// Swap the board positions of two issues (J/K reorder within a column).
    pub fn reorder(&self, key: &str, other: &str) -> Result<(), DataError> {
        let (key, other) = (key.to_string(), other.to_string());
        self.rt.block_on(self.store.call(move |conn| {
            let a = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            let b = issues::get_by_key(conn, &other)?.ok_or(cliban_core::Error::NotFound)?;
            let (pa, pb) = (a.position, b.position);
            issues::update(
                conn,
                &a,
                issues::UpdateIssue {
                    position: Some(pb),
                    ..Default::default()
                },
            )?;
            issues::update(
                conn,
                &b,
                issues::UpdateIssue {
                    position: Some(pa),
                    ..Default::default()
                },
            )?;
            Ok(())
        }))?;
        self.notify();
        Ok(())
    }

    pub fn archive(&self, key: &str) -> Result<(), DataError> {
        let key = key.to_string();
        self.rt.block_on(self.store.call(move |conn| {
            let i = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            issues::update(
                conn,
                &i,
                issues::UpdateIssue {
                    archived: Some(true),
                    ..Default::default()
                },
            )?;
            Ok(())
        }))?;
        self.notify();
        Ok(())
    }

    pub fn tag_milestone(&self, key: &str, milestone: Option<String>) -> Result<(), DataError> {
        let key = key.to_string();
        self.rt.block_on(self.store.call(move |conn| {
            let i = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            let mid = match &milestone {
                None => None,
                Some(name) => {
                    let p = projects::get_by_id(conn, i.project_id)?
                        .ok_or(cliban_core::Error::ProjectNotFound)?;
                    milestones::get(conn, &p.key, name)?.map(|m| m.id)
                }
            };
            issues::update(
                conn,
                &i,
                issues::UpdateIssue {
                    milestone_id: Some(mid),
                    ..Default::default()
                },
            )?;
            Ok(())
        }))?;
        self.notify();
        Ok(())
    }
}

impl Data {
    pub fn issue_buffer(&self, key: &str) -> Result<IssueBuffer, DataError> {
        let key = key.to_string();
        let (issue, milestone, parent) = self.rt.block_on(self.store.call(move |conn| {
            let i = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            let ms = match i.milestone_id {
                Some(m) => milestones::get_by_id(conn, m)?.map(|x| x.name),
                None => None,
            };
            let parent = match i.parent_id {
                Some(p) => issues::get_by_id(conn, p)?.map(|x| x.key),
                None => None,
            };
            Ok((i, ms, parent))
        }))?;
        Ok(IssueBuffer {
            header: format!("# Editing {} — lines above the first '---' are ignored.\n# Statuses:   backlog | in-progress | blocked | in-review | done\n# Priorities: none | low | medium | high | urgent", issue.key),
            title: issue.title,
            status: issue.status,
            priority: issue.priority,
            milestone: milestone.unwrap_or_default(),
            parent: parent.unwrap_or_default(),
            description: issue.description,
        })
    }

    pub fn apply_issue_edit(&self, key: &str, b: &IssueBuffer) -> Result<(), DataError> {
        let (key, b) = (key.to_string(), b.clone());
        self.rt.block_on(self.store.call(move |conn| {
            let cur = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            if !b.status.is_empty() && b.status != cur.status {
                issues::move_issue(conn, &cur, &b.status)?;
            }
            let project = projects::get_by_id(conn, cur.project_id)?
                .ok_or(cliban_core::Error::ProjectNotFound)?;
            let mid = if b.milestone.is_empty() {
                None
            } else {
                milestones::get(conn, &project.key, &b.milestone)?.map(|m| m.id)
            };
            let cur = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            issues::update(
                conn,
                &cur,
                issues::UpdateIssue {
                    title: Some(b.title.clone()),
                    description: Some(b.description.clone()),
                    priority: if b.priority.is_empty() {
                        None
                    } else {
                        Some(b.priority.clone())
                    },
                    milestone_id: Some(mid),
                    ..Default::default()
                },
            )?;
            Ok(())
        }))?;
        self.notify();
        Ok(())
    }

    pub fn create_issue(&self, project: &str, b: &IssueBuffer) -> Result<(), DataError> {
        let (project, b) = (project.to_string(), b.clone());
        self.rt.block_on(self.store.call(move |conn| {
            issues::create(
                conn,
                &project,
                issues::CreateIssue {
                    title: b.title.clone(),
                    description: Some(b.description.clone()),
                    status: if b.status.is_empty() {
                        None
                    } else {
                        Some(b.status.clone())
                    },
                    priority: if b.priority.is_empty() {
                        None
                    } else {
                        Some(b.priority.clone())
                    },
                    milestone: if b.milestone.is_empty() {
                        None
                    } else {
                        Some(b.milestone.clone())
                    },
                    parent_key: if b.parent.is_empty() {
                        None
                    } else {
                        Some(b.parent.clone())
                    },
                    ..Default::default()
                },
            )?;
            Ok(())
        }))?;
        self.notify();
        Ok(())
    }

    pub fn milestone_buffer(
        &self,
        project: &str,
        name: &str,
    ) -> Result<MilestoneBuffer, DataError> {
        let (project, name) = (project.to_string(), name.to_string());
        let m = self.rt.block_on(self.store.call(move |conn| {
            milestones::get(conn, &project, &name)?.ok_or(cliban_core::Error::NotFound)
        }))?;
        Ok(MilestoneBuffer {
            header: "# Editing milestone — lines above the first '---' are ignored.\n# Status: open | completed | cancelled\n# Target date: YYYY-MM-DD (empty clears it)".into(),
            name: m.name,
            status: m.status,
            target: m
                .target_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            description: m.description,
        })
    }

    pub fn apply_milestone_edit(
        &self,
        project: &str,
        orig: &str,
        b: &MilestoneBuffer,
    ) -> Result<(), DataError> {
        let (project, orig, b) = (project.to_string(), orig.to_string(), b.clone());
        self.rt.block_on(self.store.call(move |conn| {
            let m = milestones::get(conn, &project, &orig)?.ok_or(cliban_core::Error::NotFound)?;
            let target = if b.target.is_empty() {
                Some(None)
            } else {
                Some(Some(
                    chrono::NaiveDate::parse_from_str(&b.target, "%Y-%m-%d")
                        .map_err(|_| cliban_core::Error::validation("target", "want YYYY-MM-DD"))?,
                ))
            };
            milestones::update(
                conn,
                &m,
                milestones::UpdateMilestone {
                    name: if b.name != orig {
                        Some(b.name.clone())
                    } else {
                        None
                    },
                    description: Some(b.description.clone()),
                    status: if b.status.is_empty() {
                        None
                    } else {
                        Some(b.status.clone())
                    },
                    target_date: target,
                    ..Default::default()
                },
            )?;
            Ok(())
        }))?;
        self.notify();
        Ok(())
    }

    pub fn create_milestone(&self, project: &str, b: &MilestoneBuffer) -> Result<(), DataError> {
        let (project, b) = (project.to_string(), b.clone());
        self.rt.block_on(self.store.call(move |conn| {
            let target = if b.target.is_empty() {
                None
            } else {
                Some(
                    chrono::NaiveDate::parse_from_str(&b.target, "%Y-%m-%d")
                        .map_err(|_| cliban_core::Error::validation("target", "want YYYY-MM-DD"))?,
                )
            };
            milestones::create(
                conn,
                milestones::CreateMilestone {
                    project: project.clone(),
                    name: b.name.clone(),
                    description: Some(b.description.clone()),
                    target_date: target,
                    status: if b.status.is_empty() {
                        None
                    } else {
                        Some(b.status.clone())
                    },
                },
            )?;
            Ok(())
        }))?;
        self.notify();
        Ok(())
    }

    pub fn project_buffer(&self, project: &str) -> Result<ProjectBuffer, DataError> {
        let project = project.to_string();
        let p = self.rt.block_on(self.store.call(move |conn| {
            projects::get_by_key(conn, &project)?.ok_or(cliban_core::Error::ProjectNotFound)
        }))?;
        Ok(ProjectBuffer {
            header: format!(
                "# Editing project {} — the key is immutable; rename via 'name'.",
                p.key
            ),
            name: p.name,
            description: p.description,
        })
    }

    pub fn apply_project_edit(&self, project: &str, b: &ProjectBuffer) -> Result<(), DataError> {
        let (project, b) = (project.to_string(), b.clone());
        self.rt.block_on(self.store.call(move |conn| {
            let p =
                projects::get_by_key(conn, &project)?.ok_or(cliban_core::Error::ProjectNotFound)?;
            projects::update(
                conn,
                &p,
                projects::UpdateProject {
                    name: Some(b.name.clone()),
                    description: Some(b.description.clone()),
                    ..Default::default()
                },
            )?;
            Ok(())
        }))?;
        self.notify();
        Ok(())
    }
}

#[cfg(test)]
impl Data {
    pub fn open_in_memory_for_test() -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let store = Store::open_in_memory().unwrap();
        Self {
            store,
            rt,
            on_mutate: None,
        }
    }

    pub fn seed_project_issue(&self, key: &str, title: &str) {
        let (key, title) = (key.to_string(), title.to_string());
        self.rt
            .block_on(self.store.call(move |conn| {
                projects::create(
                    conn,
                    projects::CreateProject {
                        key: key.clone(),
                        name: key.clone(),
                        ..Default::default()
                    },
                )?;
                issues::create(
                    conn,
                    &key,
                    issues::CreateIssue {
                        title,
                        ..Default::default()
                    },
                )?;
                Ok(())
            }))
            .unwrap();
    }

    /// Add another issue to an existing project (seed_project_issue creates
    /// the project, so it can only be called once per project key).
    pub fn seed_issue(&self, project: &str, title: &str) {
        let (project, title) = (project.to_string(), title.to_string());
        self.rt
            .block_on(self.store.call(move |conn| {
                issues::create(
                    conn,
                    &project,
                    issues::CreateIssue {
                        title,
                        ..Default::default()
                    },
                )?;
                Ok(())
            }))
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_cards_projects_issue_to_card() {
        let d = Data::open_in_memory_for_test();
        d.seed_project_issue("CLI", "First");
        let cards = d.load_cards().unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].key, "CLI-1");
        assert_eq!(cards[0].project, "CLI");
        assert_eq!(cards[0].status, "backlog");
    }

    #[test]
    fn move_issue_changes_status() {
        let d = Data::open_in_memory_for_test();
        d.seed_project_issue("CLI", "First");
        d.move_issue("CLI-1", "in-progress").unwrap();
        assert_eq!(d.load_cards().unwrap()[0].status, "in-progress");
    }

    #[test]
    fn reorder_swaps_positions() {
        let d = Data::open_in_memory_for_test();
        d.seed_project_issue("CLI", "First"); // CLI-1
        d.rt.block_on(d.store.call(|conn| {
            issues::create(
                conn,
                "CLI",
                issues::CreateIssue {
                    title: "Second".into(),
                    ..Default::default()
                },
            )?;
            Ok(())
        }))
        .unwrap(); // CLI-2
        let pos = |cards: &[Card], k: &str| cards.iter().find(|c| c.key == k).unwrap().position;
        let before = d.load_cards().unwrap();
        d.reorder("CLI-1", "CLI-2").unwrap();
        let after = d.load_cards().unwrap();
        assert_eq!(pos(&after, "CLI-1"), pos(&before, "CLI-2"));
        assert_eq!(pos(&after, "CLI-2"), pos(&before, "CLI-1"));
    }

    #[test]
    fn archive_removes_from_board() {
        let d = Data::open_in_memory_for_test();
        d.seed_project_issue("CLI", "First");
        d.archive("CLI-1").unwrap();
        assert!(d.load_cards().unwrap().is_empty());
    }

    #[test]
    fn issue_buffer_then_apply_persists_changes() {
        let d = Data::open_in_memory_for_test();
        d.seed_project_issue("CLI", "First");
        let mut buf = d.issue_buffer("CLI-1").unwrap();
        assert_eq!(buf.title, "First");
        buf.title = "Renamed".into();
        buf.priority = "high".into();
        d.apply_issue_edit("CLI-1", &buf).unwrap();
        let cards = d.load_cards().unwrap();
        assert_eq!(cards[0].title, "Renamed");
        assert_eq!(cards[0].priority, "high");
    }

    #[test]
    fn create_milestone_then_loads() {
        let d = Data::open_in_memory_for_test();
        d.seed_project_issue("CLI", "First");
        let b = crate::buffers::MilestoneBuffer {
            name: "M1".into(),
            status: "open".into(),
            ..Default::default()
        };
        d.create_milestone("CLI", &b).unwrap();
        let ms = d.load_milestones(Some("CLI")).unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].name, "M1");
    }

    #[test]
    fn from_store_shares_the_underlying_store() {
        let store = Store::open_in_memory().unwrap();
        let d1 = Data::from_store(store.clone()).unwrap();
        d1.seed_project_issue("CLI", "First");
        // A second Data over the same Store sees the same rows.
        let d2 = Data::from_store(store).unwrap();
        assert_eq!(d2.load_cards().unwrap().len(), 1);
    }

    #[test]
    fn on_mutate_fires_after_successful_writes_only() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let mut d = Data::open_in_memory_for_test();
        d.seed_project_issue("CLI", "First");
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        d.set_on_mutate(move || {
            c.fetch_add(1, Ordering::SeqCst);
        });

        let key = d.load_cards().unwrap()[0].key.clone();
        assert_eq!(count.load(Ordering::SeqCst), 0, "reads must not notify");

        d.move_issue(&key, "done").unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        d.archive(&key).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);

        // A failed write publishes nothing.
        assert!(d.move_issue("NOPE-1", "done").is_err());
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn interleaved_field_edits_are_last_write_wins_without_corruption() {
        let store = Store::open_in_memory().unwrap();
        let d1 = Data::from_store(store.clone()).unwrap();
        d1.seed_project_issue("CLI", "First");
        let d2 = Data::from_store(store).unwrap();
        let key = d1.load_cards().unwrap()[0].key.clone();

        // Interleaved status writes: the later one sticks.
        d1.move_issue(&key, "in-progress").unwrap();
        d2.move_issue(&key, "blocked").unwrap();

        // Interleaved full edits from stale buffers: both apply cleanly,
        // the later writer's fields win, nothing is corrupted or duplicated.
        let mut b1 = d1.issue_buffer(&key).unwrap();
        let mut b2 = d2.issue_buffer(&key).unwrap();
        b1.title = "From session one".into();
        b2.title = "From session two".into();
        d1.apply_issue_edit(&key, &b1).unwrap();
        d2.apply_issue_edit(&key, &b2).unwrap();

        let cards = d1.load_cards().unwrap();
        assert_eq!(cards.len(), 1, "no ghost rows");
        assert_eq!(cards[0].title, "From session two");
        assert_eq!(cards[0].status, "blocked");
    }

    #[test]
    fn concurrent_writers_from_two_threads_leave_consistent_state() {
        let store = Store::open_in_memory().unwrap();
        let d0 = Data::from_store(store.clone()).unwrap();
        d0.seed_project_issue("CLI", "First");
        let key = d0.load_cards().unwrap()[0].key.clone();

        let writer = |a: &'static str, b: &'static str| {
            let store = store.clone();
            let key = key.clone();
            std::thread::spawn(move || {
                let d = Data::from_store(store).unwrap();
                for i in 0..20 {
                    d.move_issue(&key, if i % 2 == 0 { a } else { b }).unwrap();
                }
            })
        };
        let t1 = writer("done", "in-progress");
        let t2 = writer("blocked", "in-review");
        t1.join().unwrap();
        t2.join().unwrap();

        let cards = d0.load_cards().unwrap();
        assert_eq!(cards.len(), 1, "no duplicate/ghost rows");
        assert!(
            ["done", "in-progress", "blocked", "in-review"].contains(&cards[0].status.as_str()),
            "status must be one of the written values, got {}",
            cards[0].status
        );
    }
}
