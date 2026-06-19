//! Synchronous bridge from the TUI to the async `cliban_core::Store`.
use std::path::Path;

use cliban_core::contexts::{issues, milestones, projects};
use cliban_core::Store;

use crate::app::{Card, MilestoneRef};

pub struct Data {
    pub(crate) store: Store,
    pub(crate) rt: tokio::runtime::Runtime,
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
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| DataError(e.to_string()))?;
        let store = Store::open(path).map_err(DataError::from)?;
        Ok(Self { store, rt })
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
        let ms = self
            .rt
            .block_on(self.store.call(move |conn| milestones::list(conn, Some(&project))))?;
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
        let ps = self.rt.block_on(self.store.call(|conn| projects::list(conn)))?;
        Ok(ps.into_iter().map(|p| (p.key, p.name)).collect())
    }

    pub fn move_issue(&self, key: &str, status: &str) -> Result<(), DataError> {
        let (key, status) = (key.to_string(), status.to_string());
        self.rt.block_on(self.store.call(move |conn| {
            let i = issues::get_by_key(conn, &key)?.ok_or(cliban_core::Error::NotFound)?;
            issues::move_issue(conn, &i, &status)?;
            Ok(())
        }))?;
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
        Self { store, rt }
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
    fn archive_removes_from_board() {
        let d = Data::open_in_memory_for_test();
        d.seed_project_issue("CLI", "First");
        d.archive("CLI-1").unwrap();
        assert!(d.load_cards().unwrap().is_empty());
    }
}
