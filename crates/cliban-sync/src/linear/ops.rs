//! Typed GraphQL operations, generic over [`GraphQl`] so tests can drive them
//! with canned JSON.
//!
//! The field selections here and the structs in [`super::model`] have to agree.
//! [`ISSUE_FIELDS`] is shared by every query and mutation that returns an issue
//! so there is exactly one place where that agreement is maintained.

use serde_json::{json, Value};

use super::client::GraphQl;
use super::model::{Issue, Nodes, Team};
use crate::error::{Error, Result};

/// The issue selection, shared by every operation returning an issue.
pub const ISSUE_FIELDS: &str = r#"
    id
    identifier
    title
    description
    url
    updatedAt
    priority
    dueDate
    state { id name type position }
    team { id key name }
    labels { nodes { name } }
"#;

/// Look an issue up by its human key, e.g. `ENG-412`.
///
/// Filtering on team key + number rather than passing the identifier to
/// `issue(id:)`: the identifier form is not documented to work for every
/// workspace, whereas the filter form is plain schema.
pub async fn issue_by_key<G: GraphQl>(api: &G, team_key: &str, number: i64) -> Result<Issue> {
    let doc = format!(
        r#"query IssueByKey($team: String!, $number: Float!) {{
            issues(filter: {{ team: {{ key: {{ eq: $team }} }}, number: {{ eq: $number }} }}, first: 1) {{
                nodes {{ {ISSUE_FIELDS} }}
            }}
        }}"#
    );
    let data = api
        .query(&doc, json!({ "team": team_key, "number": number as f64 }))
        .await?;
    let nodes: Nodes<Issue> = take(data, "issues")?;
    nodes
        .first()
        .ok_or_else(|| Error::NotFound(format!("Linear issue {team_key}-{number}")))
}

/// Fetch an issue by its Linear UUID. Used on push, where the link already
/// records the stable id.
pub async fn issue_by_id<G: GraphQl>(api: &G, id: &str) -> Result<Issue> {
    let doc = format!(
        r#"query IssueById($id: String!) {{
            issue(id: $id) {{ {ISSUE_FIELDS} }}
        }}"#
    );
    let data = api.query(&doc, json!({ "id": id })).await?;
    let issue = data.get("issue").cloned().unwrap_or(Value::Null);
    if issue.is_null() {
        return Err(Error::NotFound(format!("Linear issue {id}")));
    }
    Ok(serde_json::from_value(issue)?)
}

/// A team and its workflow states, for status mapping and issue creation.
pub async fn team_by_key<G: GraphQl>(api: &G, key: &str) -> Result<Team> {
    let doc = r#"query TeamByKey($key: String!) {
        teams(filter: { key: { eq: $key } }, first: 1) {
            nodes {
                id
                key
                name
                states { nodes { id name type position } }
            }
        }
    }"#;
    let data = api.query(doc, json!({ "key": key })).await?;
    let nodes: Nodes<Team> = take(data, "teams")?;
    nodes
        .first()
        .ok_or_else(|| Error::NotFound(format!("Linear team {key}")))
}

/// An issue assigned to the viewer, with just enough cycle context to decide
/// whether `import --mine` should bring it in.
#[derive(Debug, Clone)]
pub struct AssignedIssue {
    pub issue: Issue,
    /// The cycle the issue sits in, if any.
    pub cycle_id: Option<String>,
    /// The team's currently active cycle. `None` means the team does not use
    /// cycles (or has none running), which changes what "mine, now" means.
    pub team_active_cycle_id: Option<String>,
}

impl AssignedIssue {
    /// Whether `--mine` should import this issue. A team with no active cycle
    /// puts every open assigned issue in scope; a team running a cycle scopes
    /// "mine, now" to that cycle — anything outside it is backlog.
    pub fn in_active_scope(&self) -> bool {
        match &self.team_active_cycle_id {
            None => true,
            Some(active) => self.cycle_id.as_deref() == Some(active.as_str()),
        }
    }
}

/// Every open issue assigned to the token's viewer, with cycle context.
///
/// The completed/canceled exclusion is server-side: finished work must never
/// flood in through `--mine`, however long the assignee list has grown. The
/// cycle scoping is deliberately client-side instead — whether a team uses
/// cycles is per-issue data, and the caller wants to *count* the out-of-cycle
/// skips rather than have them silently absent.
pub async fn assigned_to_viewer<G: GraphQl>(api: &G) -> Result<Vec<AssignedIssue>> {
    let doc = format!(
        r#"query ViewerAssignedIssues {{
            viewer {{
                assignedIssues(
                    filter: {{ state: {{ type: {{ nin: ["completed", "canceled"] }} }} }},
                    first: 100
                ) {{
                    nodes {{ {ISSUE_FIELDS} cycle {{ id }} team {{ activeCycle {{ id }} }} }}
                }}
            }}
        }}"#
    );
    let data = api.query(&doc, json!({})).await?;
    let nodes = data
        .pointer("/viewer/assignedIssues/nodes")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| {
            Error::Unexpected("response had no `viewer.assignedIssues.nodes` field".into())
        })?;
    nodes.into_iter().map(parse_assigned).collect()
}

/// The cycle fields ride alongside [`ISSUE_FIELDS`] in the same node; peel
/// them off before handing the rest to the shared [`Issue`] shape.
fn parse_assigned(node: Value) -> Result<AssignedIssue> {
    let cycle_id = node
        .pointer("/cycle/id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let team_active_cycle_id = node
        .pointer("/team/activeCycle/id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let issue: Issue = serde_json::from_value(node)?;
    Ok(AssignedIssue {
        issue,
        cycle_id,
        team_active_cycle_id,
    })
}

/// Fields for a new Linear issue. Only what `push --create` sets.
#[derive(Debug, Clone)]
pub struct NewIssue {
    pub team_id: String,
    pub title: String,
    pub description: String,
    pub state_id: Option<String>,
    pub priority: f64,
}

pub async fn create_issue<G: GraphQl>(api: &G, new: NewIssue) -> Result<Issue> {
    let doc = format!(
        r#"mutation IssueCreate($input: IssueCreateInput!) {{
            issueCreate(input: $input) {{
                success
                issue {{ {ISSUE_FIELDS} }}
            }}
        }}"#
    );
    let mut input = json!({
        "teamId": new.team_id,
        "title": new.title,
        "description": new.description,
        "priority": new.priority,
    });
    if let Some(state_id) = new.state_id {
        input["stateId"] = json!(state_id);
    }
    let data = api.query(&doc, json!({ "input": input })).await?;
    payload_issue(data, "issueCreate")
}

/// Fields `push` may change on an existing Linear issue. `None` means "leave
/// it alone" — the bridge never writes a field the user did not ask for.
#[derive(Debug, Clone, Default)]
pub struct IssuePatch {
    pub state_id: Option<String>,
    pub description: Option<String>,
}

impl IssuePatch {
    pub fn is_empty(&self) -> bool {
        self.state_id.is_none() && self.description.is_none()
    }
}

pub async fn update_issue<G: GraphQl>(api: &G, id: &str, patch: IssuePatch) -> Result<Issue> {
    let doc = format!(
        r#"mutation IssueUpdate($id: String!, $input: IssueUpdateInput!) {{
            issueUpdate(id: $id, input: $input) {{
                success
                issue {{ {ISSUE_FIELDS} }}
            }}
        }}"#
    );
    let mut input = json!({});
    if let Some(state_id) = patch.state_id {
        input["stateId"] = json!(state_id);
    }
    if let Some(description) = patch.description {
        input["description"] = json!(description);
    }
    let data = api.query(&doc, json!({ "id": id, "input": input })).await?;
    payload_issue(data, "issueUpdate")
}

/// Post a comment. Returns nothing useful — a comment either lands or errors.
pub async fn create_comment<G: GraphQl>(api: &G, issue_id: &str, body: &str) -> Result<()> {
    let doc = r#"mutation CommentCreate($input: CommentCreateInput!) {
        commentCreate(input: $input) { success }
    }"#;
    let data = api
        .query(
            doc,
            json!({ "input": { "issueId": issue_id, "body": body } }),
        )
        .await?;
    require_success(&data, "commentCreate")
}

/// Split `ENG-412` into `("ENG", 412)`.
///
/// Team keys can contain digits and hyphens (`WEB-2`, `A-B-12`), so the split
/// is on the *last* hyphen, and the tail has to be entirely numeric.
pub fn parse_issue_key(key: &str) -> Result<(String, i64)> {
    let trimmed = key.trim();
    let (team, number) = trimmed
        .rsplit_once('-')
        .ok_or_else(|| Error::NotFound(format!("{trimmed:?} is not a Linear issue key")))?;
    let number: i64 = number.parse().map_err(|_| {
        Error::NotFound(format!(
            "{trimmed:?} is not a Linear issue key (expected TEAM-123)"
        ))
    })?;
    if team.is_empty() {
        return Err(Error::NotFound(format!(
            "{trimmed:?} is not a Linear issue key (no team)"
        )));
    }
    Ok((team.to_uppercase(), number))
}

// ---- envelope helpers ----

fn take<T: serde::de::DeserializeOwned>(data: Value, field: &str) -> Result<T> {
    let raw = data
        .get(field)
        .cloned()
        .ok_or_else(|| Error::Unexpected(format!("response had no `{field}` field")))?;
    Ok(serde_json::from_value(raw)?)
}

/// Pull the issue out of a mutation payload, checking `success` first so a
/// refusal is reported as a refusal rather than as a parse failure.
fn payload_issue(data: Value, field: &str) -> Result<Issue> {
    require_success(&data, field)?;
    let raw = data
        .get(field)
        .and_then(|p| p.get("issue"))
        .cloned()
        .unwrap_or(Value::Null);
    if raw.is_null() {
        return Err(Error::Unexpected(format!(
            "{field} reported success but returned no issue"
        )));
    }
    Ok(serde_json::from_value(raw)?)
}

fn require_success(data: &Value, field: &str) -> Result<()> {
    let payload = data
        .get(field)
        .ok_or_else(|| Error::Unexpected(format!("response had no `{field}` field")))?;
    match payload.get("success").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        Some(false) => Err(Error::Api(vec![format!("{field} reported success: false")])),
        None => Err(Error::Unexpected(format!(
            "{field} payload had no `success` field"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Records what was asked and replies with whatever was queued.
    struct Fake {
        replies: RefCell<Vec<Value>>,
        seen: RefCell<Vec<(String, Value)>>,
    }

    impl Fake {
        fn new(replies: Vec<Value>) -> Self {
            Self {
                replies: RefCell::new(replies),
                seen: RefCell::new(Vec::new()),
            }
        }
        fn last_vars(&self) -> Value {
            self.seen.borrow().last().unwrap().1.clone()
        }
        fn last_doc(&self) -> String {
            self.seen.borrow().last().unwrap().0.clone()
        }
    }

    impl GraphQl for Fake {
        async fn query(&self, doc: &str, vars: Value) -> Result<Value> {
            self.seen.borrow_mut().push((doc.to_string(), vars));
            let mut replies = self.replies.borrow_mut();
            if replies.is_empty() {
                return Err(Error::Unexpected("fake ran out of replies".into()));
            }
            Ok(replies.remove(0))
        }
    }

    fn issue_json() -> Value {
        json!({
            "id": "uuid-1",
            "identifier": "ENG-412",
            "title": "Fix the thing",
            "description": "spec",
            "url": "https://linear.app/acme/issue/ENG-412",
            "updatedAt": "2026-07-29T12:00:00.000Z",
            "priority": 2,
            "dueDate": null,
            "state": {"id": "s1", "name": "Todo", "type": "unstarted", "position": 1.0},
            "team": {"id": "t1", "key": "ENG", "name": "Engineering"},
            "labels": {"nodes": [{"name": "bug"}]}
        })
    }

    #[test]
    fn parse_issue_key_splits_on_the_last_hyphen() {
        assert_eq!(parse_issue_key("ENG-412").unwrap(), ("ENG".into(), 412));
        // Team keys can contain hyphens; splitting on the first would break.
        assert_eq!(parse_issue_key("A-B-12").unwrap(), ("A-B".into(), 12));
        assert_eq!(parse_issue_key("  eng-7  ").unwrap(), ("ENG".into(), 7));
    }

    #[test]
    fn parse_issue_key_rejects_things_that_are_not_keys() {
        for bad in ["ENG", "ENG-", "-12", "ENG-abc", ""] {
            assert!(parse_issue_key(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[tokio::test]
    async fn issue_by_key_sends_team_and_number_and_parses_the_node() {
        let fake = Fake::new(vec![json!({"issues": {"nodes": [issue_json()]}})]);
        let issue = issue_by_key(&fake, "ENG", 412).await.unwrap();
        assert_eq!(issue.identifier, "ENG-412");
        assert_eq!(issue.label_names(), vec!["bug"]);
        let vars = fake.last_vars();
        assert_eq!(vars["team"], "ENG");
        assert_eq!(vars["number"], 412.0);
    }

    #[tokio::test]
    async fn issue_by_key_reports_not_found_for_an_empty_node_list() {
        let fake = Fake::new(vec![json!({"issues": {"nodes": []}})]);
        let err = issue_by_key(&fake, "ENG", 999).await.unwrap_err();
        assert!(err.to_string().contains("ENG-999"), "{err}");
    }

    #[tokio::test]
    async fn issue_by_id_reports_not_found_for_a_null_issue() {
        let fake = Fake::new(vec![json!({"issue": null})]);
        let err = issue_by_id(&fake, "uuid-nope").await.unwrap_err();
        assert!(err.to_string().contains("uuid-nope"), "{err}");
    }

    #[tokio::test]
    async fn team_by_key_parses_the_state_list() {
        let fake = Fake::new(vec![json!({"teams": {"nodes": [{
            "id": "t1", "key": "ENG", "name": "Engineering",
            "states": {"nodes": [
                {"id": "s1", "name": "Todo", "type": "unstarted", "position": 1.0},
                {"id": "s2", "name": "Done", "type": "completed", "position": 2.0}
            ]}
        }]}})]);
        let team = team_by_key(&fake, "ENG").await.unwrap();
        assert_eq!(team.states.nodes.len(), 2);
        assert_eq!(team.states.nodes[1].name, "Done");
    }

    #[tokio::test]
    async fn update_issue_omits_fields_the_caller_left_unset() {
        let fake = Fake::new(vec![
            json!({"issueUpdate": {"success": true, "issue": issue_json()}}),
        ]);
        let patch = IssuePatch {
            state_id: Some("s2".into()),
            description: None,
        };
        update_issue(&fake, "uuid-1", patch).await.unwrap();
        let input = &fake.last_vars()["input"];
        assert_eq!(input["stateId"], "s2");
        assert!(
            input.get("description").is_none(),
            "an unset field must not be sent: {input}"
        );
    }

    #[tokio::test]
    async fn create_issue_sends_the_state_only_when_given() {
        let fake = Fake::new(vec![
            json!({"issueCreate": {"success": true, "issue": issue_json()}}),
        ]);
        create_issue(
            &fake,
            NewIssue {
                team_id: "t1".into(),
                title: "T".into(),
                description: "D".into(),
                state_id: None,
                priority: 2.0,
            },
        )
        .await
        .unwrap();
        let input = &fake.last_vars()["input"];
        assert_eq!(input["teamId"], "t1");
        assert!(input.get("stateId").is_none());
    }

    #[tokio::test]
    async fn a_mutation_reporting_success_false_is_an_error() {
        let fake = Fake::new(vec![
            json!({"issueUpdate": {"success": false, "issue": null}}),
        ]);
        let err = update_issue(&fake, "uuid-1", IssuePatch::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("success: false"), "{err}");
    }

    #[tokio::test]
    async fn success_true_with_no_issue_is_reported_rather_than_parsed_as_garbage() {
        let fake = Fake::new(vec![
            json!({"issueCreate": {"success": true, "issue": null}}),
        ]);
        let err = create_issue(
            &fake,
            NewIssue {
                team_id: "t1".into(),
                title: "T".into(),
                description: "D".into(),
                state_id: None,
                priority: 0.0,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("returned no issue"), "{err}");
    }

    #[tokio::test]
    async fn create_comment_checks_success() {
        let fake = Fake::new(vec![json!({"commentCreate": {"success": true}})]);
        create_comment(&fake, "uuid-1", "hello").await.unwrap();
        assert_eq!(fake.last_vars()["input"]["issueId"], "uuid-1");
        assert_eq!(fake.last_vars()["input"]["body"], "hello");
        assert!(fake.last_doc().contains("commentCreate"));
    }

    fn assigned_node(identifier: &str, cycle: Option<&str>, active: Option<&str>) -> Value {
        let mut node = issue_json();
        node["identifier"] = json!(identifier);
        node["cycle"] = match cycle {
            Some(id) => json!({ "id": id }),
            None => Value::Null,
        };
        node["team"]["activeCycle"] = match active {
            Some(id) => json!({ "id": id }),
            None => Value::Null,
        };
        node
    }

    #[tokio::test]
    async fn assigned_to_viewer_parses_issues_with_their_cycle_context() {
        let fake = Fake::new(vec![json!({"viewer": {"assignedIssues": {"nodes": [
            assigned_node("ENG-1", Some("cyc-9"), Some("cyc-9")),
            assigned_node("ENG-2", None, None),
        ]}}})]);
        let assigned = assigned_to_viewer(&fake).await.unwrap();
        assert_eq!(assigned.len(), 2);
        assert_eq!(assigned[0].issue.identifier, "ENG-1");
        assert_eq!(assigned[0].cycle_id.as_deref(), Some("cyc-9"));
        assert_eq!(assigned[0].team_active_cycle_id.as_deref(), Some("cyc-9"));
        assert_eq!(assigned[1].cycle_id, None);
        assert_eq!(assigned[1].team_active_cycle_id, None);
    }

    #[tokio::test]
    async fn assigned_to_viewer_asks_only_for_open_states() {
        // The server-side filter is the contract: completed and canceled work
        // must never flood in through --mine.
        let fake = Fake::new(vec![json!({"viewer": {"assignedIssues": {"nodes": []}}})]);
        assigned_to_viewer(&fake).await.unwrap();
        let doc = fake.last_doc();
        assert!(doc.contains("assignedIssues"), "{doc}");
        assert!(doc.contains("completed"), "{doc}");
        assert!(doc.contains("canceled"), "{doc}");
        assert!(doc.contains("nin"), "{doc}");
    }

    #[tokio::test]
    async fn assigned_to_viewer_reports_a_missing_viewer_rather_than_panicking() {
        let fake = Fake::new(vec![json!({"nonsense": 1})]);
        let err = assigned_to_viewer(&fake).await.unwrap_err();
        assert!(err.to_string().contains("viewer"), "{err}");
    }

    #[test]
    fn in_active_scope_is_everything_when_the_team_has_no_active_cycle() {
        // A team that does not use cycles reports no active cycle; every open
        // assigned issue is then in scope.
        for cycle in [None, Some("cyc-1".to_string())] {
            let a = AssignedIssue {
                issue: serde_json::from_value(issue_json()).unwrap(),
                cycle_id: cycle,
                team_active_cycle_id: None,
            };
            assert!(a.in_active_scope());
        }
    }

    #[test]
    fn in_active_scope_requires_the_active_cycle_when_the_team_uses_cycles() {
        let make = |cycle: Option<&str>| AssignedIssue {
            issue: serde_json::from_value(issue_json()).unwrap(),
            cycle_id: cycle.map(str::to_string),
            team_active_cycle_id: Some("cyc-active".into()),
        };
        assert!(make(Some("cyc-active")).in_active_scope());
        assert!(!make(Some("cyc-old")).in_active_scope(), "a past cycle is backlog, not now");
        assert!(!make(None).in_active_scope(), "no cycle at all means backlog on a cycle team");
    }

    #[test]
    fn issue_fields_selects_everything_the_model_requires() {
        // `model::Issue` has no `#[serde(default)]` on these, so a selection
        // that dropped one would fail at runtime on every operation at once.
        for field in [
            "id",
            "identifier",
            "title",
            "description",
            "url",
            "updatedAt",
            "priority",
            "dueDate",
            "state {",
            "team {",
            "labels {",
        ] {
            assert!(
                ISSUE_FIELDS.contains(field),
                "{field} missing from ISSUE_FIELDS"
            );
        }
    }
}
