//! End-to-end tests for `cliban import linear` and `cliban push linear`.
//!
//! The bridge is driven against a local GraphQL stub rather than api.linear.app,
//! so the suite is hermetic: no token, no network, no rate limit. The stub is a
//! bare `TcpListener` speaking just enough HTTP/1.1 to satisfy reqwest, which is
//! cheaper than adding an HTTP server dependency for six tests.
//!
//! What is worth testing here rather than in `cliban-sync`'s unit tests is the
//! part that spans the seam: that an import writes a real issue through the real
//! contexts, and — the guarantee the whole design rests on — that re-importing
//! does not eat a plan an agent has been ticking.

#![cfg(feature = "linear")]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------- the stub

/// Replies keyed by GraphQL operation name, served for as long as the stub runs.
type Replies = HashMap<String, String>;

struct Stub {
    endpoint: String,
    /// (operation name, raw request body) for every request, in order.
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl Stub {
    fn start(replies: Replies) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_thread: Arc<Mutex<Vec<(String, String)>>> = Arc::clone(&seen);

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let body = match read_request_body(&mut stream) {
                    Some(b) => b,
                    None => continue,
                };
                let op = operation_name(&body).unwrap_or_default();
                seen_thread.lock().unwrap().push((op.clone(), body.clone()));
                // A reply keyed `"Op:<variables.id>"` wins over one keyed by
                // the bare operation, so one stub can serve several issues.
                let reply = request_id(&body)
                    .and_then(|id| replies.get(&format!("{op}:{id}")).cloned())
                    .or_else(|| replies.get(&op).cloned())
                    // An unexpected operation must fail loudly as a GraphQL
                    // error, not hang or return something plausible.
                    .unwrap_or_else(|| {
                        format!(r#"{{"errors":[{{"message":"stub has no reply for {op:?}"}}]}}"#)
                    });
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.len(),
                    reply
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        Stub {
            endpoint: format!("http://127.0.0.1:{port}"),
            seen,
        }
    }

    fn operations(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|(op, _)| op.clone())
            .collect()
    }

    /// Raw request bodies of every call to `op`, in order.
    fn requests_for(&self, op: &str) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(o, _)| o == op)
            .map(|(_, body)| body.clone())
            .collect()
    }
}

/// Read headers, honour `Content-Length`, return the body.
fn read_request_body(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    // Headers, one byte at a time — slow and completely adequate here.
    while !buf.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => return None,
            Ok(_) => buf.push(byte[0]),
        }
    }
    let headers = String::from_utf8_lossy(&buf).to_lowercase();
    let len: usize = headers
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).ok()?;
    Some(String::from_utf8_lossy(&body).to_string())
}

/// `variables.id` when the request has one, for per-issue reply keys.
fn request_id(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("variables")?
        .get("id")?
        .as_str()
        .map(str::to_string)
}

/// `{"query":"query IssueByKey($team..."}` → `IssueByKey`.
fn operation_name(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let query = v.get("query")?.as_str()?;
    for keyword in ["query ", "mutation "] {
        if let Some(rest) = query.trim_start().strip_prefix(keyword) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

// ---------------------------------------------------------------- fixtures

fn issue_reply(title: &str, state: (&str, &str), updated_at: &str) -> String {
    serde_json::json!({
        "data": { "issues": { "nodes": [ linear_issue(title, state, updated_at) ] } }
    })
    .to_string()
}

fn issue_by_id_reply(title: &str, state: (&str, &str), updated_at: &str) -> String {
    serde_json::json!({
        "data": { "issue": linear_issue(title, state, updated_at) }
    })
    .to_string()
}

fn linear_issue(title: &str, state: (&str, &str), updated_at: &str) -> serde_json::Value {
    linear_issue_with("linear-uuid-1", "ENG-412", title, state, updated_at)
}

fn linear_issue_with(
    id: &str,
    identifier: &str,
    title: &str,
    state: (&str, &str),
    updated_at: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "identifier": identifier,
        "title": title,
        "description": "The upstream spec text.",
        "url": format!("https://linear.app/acme/issue/{identifier}"),
        "updatedAt": updated_at,
        "priority": 2,
        "dueDate": "2026-08-15",
        "state": {"id": "state-todo", "name": state.0, "type": state.1, "position": 1.0},
        "team": {"id": "team-1", "key": "ENG", "name": "Engineering"},
        "labels": {"nodes": [{"name": "bug"}]}
    })
}

fn team_reply() -> String {
    serde_json::json!({
        "data": { "teams": { "nodes": [{
            "id": "team-1", "key": "ENG", "name": "Engineering",
            "states": { "nodes": [
                {"id": "state-todo", "name": "Todo", "type": "unstarted", "position": 1.0},
                {"id": "state-prog", "name": "In Progress", "type": "started", "position": 2.0},
                {"id": "state-rev", "name": "In Review", "type": "started", "position": 3.0},
                {"id": "state-done", "name": "Done", "type": "completed", "position": 4.0}
            ]}
        }]}}
    })
    .to_string()
}

fn import_replies(title: &str, state: (&str, &str), updated_at: &str) -> Replies {
    HashMap::from([(
        "IssueByKey".to_string(),
        issue_reply(title, state, updated_at),
    )])
}

fn comment_create_reply(comment_id: &str) -> String {
    serde_json::json!({"data": {"commentCreate": {
        "success": true, "comment": {"id": comment_id}
    }}})
    .to_string()
}

fn comment_update_ok_reply() -> String {
    serde_json::json!({"data": {"commentUpdate": {"success": true}}}).to_string()
}

/// One node of the `--mine` query: an issue plus its cycle context.
fn assigned_node(
    mut issue: serde_json::Value,
    cycle: Option<&str>,
    active_cycle: Option<&str>,
) -> serde_json::Value {
    issue["cycle"] = match cycle {
        Some(id) => serde_json::json!({ "id": id }),
        None => serde_json::Value::Null,
    };
    issue["team"]["activeCycle"] = match active_cycle {
        Some(id) => serde_json::json!({ "id": id }),
        None => serde_json::Value::Null,
    };
    issue
}

fn viewer_reply(nodes: Vec<serde_json::Value>) -> String {
    serde_json::json!({
        "data": { "viewer": { "assignedIssues": { "nodes": nodes } } }
    })
    .to_string()
}

// ---------------------------------------------------------------- harness

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Board {
    db: String,
}

impl Board {
    /// A fresh temp DB with one project.
    fn new(tag: &str) -> Board {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let db = std::env::temp_dir()
            .join(format!("cliban_linear_{tag}_{nanos}_{n}.db"))
            .to_string_lossy()
            .to_string();
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{db}{suffix}"));
        }
        let board = Board { db };
        board
            .run(&["project", "add", "PROJ", "--name", "Demo"], None)
            .assert_ok();
        board
    }

    /// Run the binary against this board with a clean environment, so a
    /// developer's own `CLIBAN_DB` or `LINEAR_API_KEY` cannot leak in.
    fn run(&self, args: &[&str], stub: Option<&Stub>) -> Run {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cliban"));
        cmd.env_clear()
            .env("HOME", std::env::temp_dir())
            // No config file exists at this HOME, which is the default the
            // bridge is meant to work under.
            .env(
                "XDG_CONFIG_HOME",
                std::env::temp_dir().join("cliban-no-config"),
            )
            .env("CLIBAN_DB", &self.db)
            .args(args);
        if let Some(stub) = stub {
            cmd.env("LINEAR_API_KEY", "lin_api_test")
                .env("CLIBAN_LINEAR_ENDPOINT", &stub.endpoint);
        }
        let out = cmd.output().expect("run cliban");
        Run {
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            code: out.status.code().unwrap_or(-1),
        }
    }

    /// The issue as JSON, via the CLI's own read path.
    fn show(&self, key: &str) -> serde_json::Value {
        let run = self.run(&["issue", "show", key, "--json"], None);
        run.assert_ok();
        serde_json::from_str(&run.stdout).expect("issue show --json")
    }
}

impl Drop for Board {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.db));
        }
    }
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

impl Run {
    fn assert_ok(&self) {
        assert_eq!(
            self.code, 0,
            "expected success, got {}\nstdout: {}\nstderr: {}",
            self.code, self.stdout, self.stderr
        );
    }
}

// ---------------------------------------------------------------- tests

#[test]
fn import_creates_a_cliban_issue_from_a_linear_issue() {
    let board = Board::new("import");
    let stub = Stub::start(import_replies(
        "Fix the flaky thing",
        ("In Progress", "started"),
        "2026-07-29T12:00:00.000Z",
    ));

    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub),
    );
    run.assert_ok();

    let out: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(out["action"], "imported");
    assert_eq!(out["linear"], "ENG-412");
    let key = out["cliban"].as_str().unwrap().to_string();

    let issue = board.show(&key);
    assert_eq!(issue["title"], "Fix the flaky thing");
    assert_eq!(issue["status"], "in-progress", "state name maps to status");
    assert_eq!(issue["priority"], "high", "Linear priority 2 is high");
    assert_eq!(issue["due_date"], "2026-08-15");
    assert_eq!(issue["labels"], serde_json::json!(["bug"]));

    let description = issue["description"].as_str().unwrap();
    assert!(description.contains("## Spec"));
    assert!(description.contains("The upstream spec text."));
    assert!(
        description.contains("https://linear.app/acme/issue/ENG-412"),
        "provenance link is missing"
    );
    assert!(
        description.contains("## Plan"),
        "tick needs a ## Plan section to exist from the start"
    );
}

#[test]
fn reimport_refreshes_the_spec_and_preserves_a_ticked_plan() {
    let board = Board::new("refresh");

    let stub = Stub::start(import_replies(
        "Original title",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub),
    );
    run.assert_ok();
    let key = serde_json::from_str::<serde_json::Value>(&run.stdout).unwrap()["cliban"]
        .as_str()
        .unwrap()
        .to_string();

    // An agent writes a plan and ticks a step — the state that must survive.
    let plan = "## Spec\n\nThe upstream spec text.\n\n## Plan\n\n\
                ### Task 1: wire it up\n\n\
                - [ ] **Step 1: write the client**\n\
                - [ ] **Step 2: write the tests**\n";
    board
        .run(&["issue", "edit", &key, "--description", plan], None)
        .assert_ok();
    board
        .run(&["issue", "tick", &key, "--task", "1", "--step", "1"], None)
        .assert_ok();
    board
        .run(&["issue", "log", &key, "client landed, tests next"], None)
        .assert_ok();

    // Re-import with a changed upstream title.
    let stub2 = Stub::start(import_replies(
        "Renamed upstream",
        ("In Review", "started"),
        "2026-07-30T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub2),
    );
    run.assert_ok();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&run.stdout).unwrap()["action"],
        "refreshed",
        "the second import should refresh, not create a duplicate"
    );

    let issue = board.show(&key);
    assert_eq!(issue["title"], "Renamed upstream", "Linear owns the title");
    assert_eq!(issue["status"], "in-review");

    let description = issue["description"].as_str().unwrap();
    assert!(
        description.contains("- [x] **Step 1: write the client**"),
        "the ticked step was lost — this is the guarantee the design rests on:\n{description}"
    );
    assert!(
        description.contains("- [ ] **Step 2: write the tests**"),
        "the untouched step was lost:\n{description}"
    );
    assert!(
        description.contains("client landed, tests next"),
        "the logged note was lost:\n{description}"
    );
    assert!(
        description.contains("### Task 1: wire it up"),
        "the task heading was lost:\n{description}"
    );
}

#[test]
fn a_second_import_does_not_create_a_second_issue() {
    let board = Board::new("dupe");
    for _ in 0..2 {
        let stub = Stub::start(import_replies(
            "Same issue",
            ("Todo", "unstarted"),
            "2026-07-29T12:00:00.000Z",
        ));
        board
            .run(
                &["import", "linear", "ENG-412", "--project", "PROJ"],
                Some(&stub),
            )
            .assert_ok();
    }
    let run = board.run(&["issue", "ls", "--project", "PROJ", "--json"], None);
    run.assert_ok();
    let count = run.stdout.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(count, 1, "importing twice created {count} issues");
}

#[test]
fn a_cancelled_linear_issue_is_archived_rather_than_deleted() {
    let board = Board::new("cancelled");
    let stub = Stub::start(import_replies(
        "Abandoned work",
        ("Canceled", "canceled"),
        "2026-07-29T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub),
    );
    run.assert_ok();
    let out: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(out["status"], "done");
    assert_eq!(out["archived"], true, "cliban archives, it never deletes");
}

#[test]
fn import_dry_run_writes_nothing() {
    let board = Board::new("dryrun");
    let stub = Stub::start(import_replies(
        "Not yet imported",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    let run = board.run(
        &[
            "import",
            "linear",
            "ENG-412",
            "--project",
            "PROJ",
            "--dry-run",
        ],
        Some(&stub),
    );
    run.assert_ok();
    assert!(run.stdout.contains("dry run"), "{}", run.stdout);
    assert!(run.stdout.contains("Not yet imported"), "{}", run.stdout);

    let ls = board.run(&["issue", "ls", "--project", "PROJ", "--json"], None);
    ls.assert_ok();
    assert!(
        ls.stdout.trim().is_empty(),
        "dry run created an issue: {}",
        ls.stdout
    );
}

#[test]
fn push_moves_the_linear_state_and_posts_a_comment() {
    let board = Board::new("push");

    let stub = Stub::start(import_replies(
        "Work to do",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub),
    );
    run.assert_ok();
    let key = serde_json::from_str::<serde_json::Value>(&run.stdout).unwrap()["cliban"]
        .as_str()
        .unwrap()
        .to_string();

    board
        .run(&["issue", "mv", &key, "in-review"], None)
        .assert_ok();

    let push_stub = Stub::start(HashMap::from([
        (
            "IssueById".to_string(),
            issue_by_id_reply(
                "Work to do",
                ("Todo", "unstarted"),
                "2026-07-29T12:00:00.000Z",
            ),
        ),
        ("TeamByKey".to_string(), team_reply()),
        (
            "IssueUpdate".to_string(),
            serde_json::json!({"data": {"issueUpdate": {"success": true,
                "issue": linear_issue("Work to do", ("In Review", "started"),
                                      "2026-07-29T13:00:00.000Z")}}})
            .to_string(),
        ),
        (
            "CommentCreate".to_string(),
            comment_create_reply("comment-uuid-1"),
        ),
    ]));

    let run = board.run(&["push", "linear", &key, "--json"], Some(&push_stub));
    run.assert_ok();
    let out: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(out["action"], "pushed");
    assert_eq!(out["linear"], "ENG-412");
    assert_eq!(out["state"], "In Review", "in-review mapped to the column");
    assert_eq!(out["wrote"], serde_json::json!(["state", "comment"]));

    let ops = push_stub.operations();
    assert!(ops.contains(&"IssueUpdate".to_string()), "{ops:?}");
    assert!(
        ops.contains(&"CommentCreate".to_string()),
        "the default push posts a comment: {ops:?}"
    );
}

#[test]
fn push_refuses_when_linear_moved_since_the_last_sync() {
    let board = Board::new("stale");
    let stub = Stub::start(import_replies(
        "Work to do",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub),
    );
    run.assert_ok();
    let key = serde_json::from_str::<serde_json::Value>(&run.stdout).unwrap()["cliban"]
        .as_str()
        .unwrap()
        .to_string();

    // Move it locally, so a successful push has a real state change to make and
    // the --force assertion below is testing something.
    board.run(&["issue", "mv", &key, "done"], None).assert_ok();

    // Upstream has moved on since the import stamped `remote_updated_at`.
    let stale_replies = HashMap::from([
        (
            "IssueById".to_string(),
            issue_by_id_reply(
                "Renamed by a human",
                ("Todo", "unstarted"),
                "2026-07-31T09:00:00.000Z",
            ),
        ),
        ("TeamByKey".to_string(), team_reply()),
        (
            "IssueUpdate".to_string(),
            serde_json::json!({"data": {"issueUpdate": {"success": true,
                "issue": linear_issue("Renamed by a human", ("Done", "completed"),
                                      "2026-07-31T10:00:00.000Z")}}})
            .to_string(),
        ),
        (
            "CommentCreate".to_string(),
            comment_create_reply("comment-uuid-1"),
        ),
    ]);

    let stub2 = Stub::start(stale_replies.clone());
    let run = board.run(&["push", "linear", &key], Some(&stub2));
    assert_eq!(run.code, 2, "stale write should exit 2: {run:?}");
    assert!(
        run.stderr.contains("changed in Linear"),
        "stderr should explain: {}",
        run.stderr
    );
    assert!(run.stderr.contains("--force"), "and offer the way out");
    let ops = stub2.operations();
    assert!(
        !ops.contains(&"IssueUpdate".to_string()),
        "nothing should have been written: {ops:?}"
    );

    // --force is the documented override.
    let stub3 = Stub::start(stale_replies);
    board
        .run(&["push", "linear", &key, "--force"], Some(&stub3))
        .assert_ok();
    assert!(stub3.operations().contains(&"IssueUpdate".to_string()));
}

#[test]
fn push_dry_run_writes_nothing_and_shows_the_comment() {
    let board = Board::new("pushdry");
    let stub = Stub::start(import_replies(
        "Work to do",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub),
    );
    run.assert_ok();
    let key = serde_json::from_str::<serde_json::Value>(&run.stdout).unwrap()["cliban"]
        .as_str()
        .unwrap()
        .to_string();

    let push_stub = Stub::start(HashMap::from([
        (
            "IssueById".to_string(),
            issue_by_id_reply(
                "Work to do",
                ("Todo", "unstarted"),
                "2026-07-29T12:00:00.000Z",
            ),
        ),
        ("TeamByKey".to_string(), team_reply()),
    ]));
    let run = board.run(&["push", "linear", &key, "--dry-run"], Some(&push_stub));
    run.assert_ok();
    assert!(run.stdout.contains("dry run"), "{}", run.stdout);
    assert!(run.stdout.contains("comment:"), "{}", run.stdout);
    // The imported issue is still where Linear has it, so there is no state
    // move to make and the dry run should say so rather than implying a write.
    assert!(
        run.stdout.contains("already Todo, no change"),
        "{}",
        run.stdout
    );
    let ops = push_stub.operations();
    assert!(
        !ops.contains(&"IssueUpdate".to_string()) && !ops.contains(&"CommentCreate".to_string()),
        "dry run performed a mutation: {ops:?}"
    );
}

// ---- per-link origin: spec ownership follows who created the pairing ----

#[test]
fn reimport_over_a_pushed_origin_link_keeps_the_local_spec() {
    let board = Board::new("pushedorigin");

    // The issue is born on the board, spec and all — cliban owns that spec.
    board
        .run(
            &["issue", "add", "--project", "PROJ", "--title", "Local work"],
            None,
        )
        .assert_ok();
    let description = "## Spec\n\nThe LOCAL spec, written on the board.\n\n## Plan\n\n\
                       ### Task 1: do it\n\n- [ ] **Step 1: start**\n";
    board
        .run(
            &["issue", "edit", "PROJ-1", "--description", description],
            None,
        )
        .assert_ok();

    // push --create is what stamps the link's origin as 'pushed'.
    let create_stub = Stub::start(HashMap::from([
        ("TeamByKey".to_string(), team_reply()),
        (
            "IssueCreate".to_string(),
            serde_json::json!({"data": {"issueCreate": {"success": true,
                "issue": linear_issue("Local work", ("Todo", "unstarted"),
                                      "2026-07-29T12:00:00.000Z")}}})
            .to_string(),
        ),
        (
            "CommentCreate".to_string(),
            comment_create_reply("comment-uuid-1"),
        ),
    ]));
    board
        .run(
            &["push", "linear", "PROJ-1", "--create", "--team", "ENG"],
            Some(&create_stub),
        )
        .assert_ok();

    // Re-import: Linear still owns the title, but the spec stays local.
    let stub = Stub::start(import_replies(
        "Renamed upstream",
        ("In Progress", "started"),
        "2026-07-30T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub),
    );
    run.assert_ok();
    let out: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(out["action"], "refreshed");
    assert!(
        !run.stderr.contains("local edits"),
        "a locally owned spec must not read as drift: {}",
        run.stderr
    );

    let issue = board.show("PROJ-1");
    assert_eq!(
        issue["title"], "Renamed upstream",
        "Linear still owns the title, whatever the origin"
    );
    let description = issue["description"].as_str().unwrap();
    assert!(
        description.contains("The LOCAL spec, written on the board."),
        "a pushed-origin link means cliban owns the spec — the re-import ate it:\n{description}"
    );
    assert!(
        !description.contains("The upstream spec text."),
        "the upstream description must not replace a cliban-owned spec:\n{description}"
    );
    assert!(
        description.contains("- [ ] **Step 1: start**"),
        "the plan must survive as always:\n{description}"
    );
}

#[test]
fn reimport_over_an_imported_origin_link_still_refreshes_the_spec() {
    let board = Board::new("importedorigin");
    let stub = Stub::start(import_replies(
        "Upstream work",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub),
    );
    run.assert_ok();
    let key = serde_json::from_str::<serde_json::Value>(&run.stdout).unwrap()["cliban"]
        .as_str()
        .unwrap()
        .to_string();

    // Someone edits the spec locally. For an imported-origin link that edit
    // does not stick — Linear owns the spec, exactly as before this feature.
    let edited = "## Spec\n\nLOCAL EDIT that must not survive\n\n## Plan\n\n\
                  ### Task 1: t\n\n- [ ] **Step 1: s**\n";
    board
        .run(&["issue", "edit", &key, "--description", edited], None)
        .assert_ok();

    let stub2 = Stub::start(import_replies(
        "Upstream work",
        ("Todo", "unstarted"),
        "2026-07-30T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub2),
    );
    run.assert_ok();
    assert!(
        run.stderr.contains("local edits"),
        "the overwrite should be announced first: {}",
        run.stderr
    );

    let description = board.show(&key)["description"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        description.contains("The upstream spec text."),
        "imported-origin spec must refresh from Linear:\n{description}"
    );
    assert!(
        !description.contains("LOCAL EDIT that must not survive"),
        "the local edit should have been overwritten:\n{description}"
    );
}

// ---- the living progress comment: one comment, edited in place ----

/// Replies for a push over an already-imported link whose upstream has not
/// moved: issue lookup, team lookup, and both comment mutations.
fn living_comment_replies(create_id: &str) -> Replies {
    HashMap::from([
        (
            "IssueById".to_string(),
            issue_by_id_reply(
                "Work to do",
                ("Todo", "unstarted"),
                "2026-07-29T12:00:00.000Z",
            ),
        ),
        ("TeamByKey".to_string(), team_reply()),
        ("CommentCreate".to_string(), comment_create_reply(create_id)),
        ("CommentUpdate".to_string(), comment_update_ok_reply()),
    ])
}

/// Import ENG-412 into a fresh board and return the cliban key.
fn import_eng412(board: &Board) -> String {
    let stub = Stub::start(import_replies(
        "Work to do",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    let run = board.run(
        &["import", "linear", "ENG-412", "--project", "PROJ", "--json"],
        Some(&stub),
    );
    run.assert_ok();
    serde_json::from_str::<serde_json::Value>(&run.stdout).unwrap()["cliban"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_second_push_updates_the_comment_instead_of_appending_another() {
    let board = Board::new("living");
    let key = import_eng412(&board);

    // First push: no comment exists yet, so it is created.
    let stub1 = Stub::start(living_comment_replies("comment-uuid-1"));
    board
        .run(&["push", "linear", &key], Some(&stub1))
        .assert_ok();
    let ops = stub1.operations();
    assert!(ops.contains(&"CommentCreate".to_string()), "{ops:?}");
    assert!(
        !ops.contains(&"CommentUpdate".to_string()),
        "nothing to update yet: {ops:?}"
    );

    // Second push: the recorded comment is edited in place.
    let stub2 = Stub::start(living_comment_replies("comment-uuid-never-used"));
    board
        .run(&["push", "linear", &key], Some(&stub2))
        .assert_ok();
    let ops = stub2.operations();
    assert!(
        ops.contains(&"CommentUpdate".to_string()),
        "the second push must edit, not append: {ops:?}"
    );
    assert!(
        !ops.contains(&"CommentCreate".to_string()),
        "the second push must not create a second comment: {ops:?}"
    );
    let update = &stub2.requests_for("CommentUpdate")[0];
    assert!(
        update.contains("comment-uuid-1"),
        "the update must address the comment the first push created: {update}"
    );
}

#[test]
fn the_digest_reflects_ticked_steps_logged_findings_and_test_status() {
    let board = Board::new("digest");
    let key = import_eng412(&board);

    let plan = "## Spec\n\nThe upstream spec text.\n\n## Plan\n\n\
                ### Task 1: wire it up\n\n\
                - [ ] **Step 1: write the client**\n\
                - [ ] **Step 2: write the tests**\n";
    board
        .run(&["issue", "edit", &key, "--description", plan], None)
        .assert_ok();
    board
        .run(&["issue", "tick", &key, "--task", "1", "--step", "1"], None)
        .assert_ok();
    for finding in [
        "an old finding that should age out",
        "second finding",
        "third finding",
        "client wired; suite 12 passed / 0 failed",
    ] {
        board
            .run(&["issue", "log", &key, finding], None)
            .assert_ok();
    }

    let stub = Stub::start(living_comment_replies("comment-uuid-1"));
    board
        .run(&["push", "linear", &key], Some(&stub))
        .assert_ok();

    let body = stub.requests_for("CommentCreate").remove(0);
    assert!(
        body.contains("Plan: 1/2 steps"),
        "plan progress missing from the digest: {body}"
    );
    assert!(
        body.contains("client wired; suite 12 passed / 0 failed"),
        "the newest finding is missing: {body}"
    );
    assert!(
        body.contains("**Tests:** 12 passed / 0 failed"),
        "the test status a finding carried should be pulled out: {body}"
    );
    assert!(
        !body.contains("an old finding that should age out"),
        "only the last few findings belong in the digest: {body}"
    );
    assert!(
        body.contains("maintained by cliban"),
        "the footer is the reader's explanation for the edits: {body}"
    );
}

#[test]
fn a_deleted_comment_is_recreated_once_and_the_new_id_sticks() {
    let board = Board::new("recreate");
    let key = import_eng412(&board);

    // Push 1 creates comment-uuid-1.
    let stub1 = Stub::start(living_comment_replies("comment-uuid-1"));
    board
        .run(&["push", "linear", &key], Some(&stub1))
        .assert_ok();

    // Push 2: someone deleted the comment in Linear. The update resolves to
    // not-found, and the push recovers by creating a fresh comment.
    let stub2 = Stub::start(HashMap::from([
        (
            "IssueById".to_string(),
            issue_by_id_reply(
                "Work to do",
                ("Todo", "unstarted"),
                "2026-07-29T12:00:00.000Z",
            ),
        ),
        ("TeamByKey".to_string(), team_reply()),
        (
            "CommentUpdate".to_string(),
            r#"{"errors":[{"message":"Entity not found: Comment"}]}"#.to_string(),
        ),
        (
            "CommentCreate".to_string(),
            comment_create_reply("comment-uuid-2"),
        ),
    ]));
    board
        .run(&["push", "linear", &key], Some(&stub2))
        .assert_ok();
    let ops = stub2.operations();
    let update_pos = ops.iter().position(|o| o == "CommentUpdate");
    let create_pos = ops.iter().position(|o| o == "CommentCreate");
    assert!(
        update_pos.is_some() && create_pos.is_some() && update_pos < create_pos,
        "recovery is try-update, then create: {ops:?}"
    );

    // Push 3 proves the recreated id was stored: the update addresses it.
    let stub3 = Stub::start(living_comment_replies("comment-uuid-never-used"));
    board
        .run(&["push", "linear", &key], Some(&stub3))
        .assert_ok();
    assert!(
        !stub3.operations().contains(&"CommentCreate".to_string()),
        "the recreated comment must be reused, not recreated again: {:?}",
        stub3.operations()
    );
    let update = &stub3.requests_for("CommentUpdate")[0];
    assert!(
        update.contains("comment-uuid-2"),
        "the stored id should be the recreated one: {update}"
    );
}

// ---- sync linear: refresh every linked issue in one call ----

fn import_replies_for(issue: serde_json::Value) -> Replies {
    HashMap::from([(
        "IssueByKey".to_string(),
        serde_json::json!({"data": {"issues": {"nodes": [issue]}}}).to_string(),
    )])
}

fn issue_by_id_reply_for(issue: serde_json::Value) -> String {
    serde_json::json!({"data": {"issue": issue}}).to_string()
}

/// Import ENG-412 (uuid-1) and ENG-9 (uuid-9) into PROJ as PROJ-1 / PROJ-2.
fn board_with_two_linked_issues(tag: &str) -> Board {
    let board = Board::new(tag);
    let stub = Stub::start(import_replies(
        "First issue",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    board
        .run(
            &["import", "linear", "ENG-412", "--project", "PROJ"],
            Some(&stub),
        )
        .assert_ok();
    let stub = Stub::start(import_replies_for(linear_issue_with(
        "linear-uuid-9",
        "ENG-9",
        "Second issue",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    )));
    board
        .run(
            &["import", "linear", "ENG-9", "--project", "PROJ"],
            Some(&stub),
        )
        .assert_ok();
    board
}

#[test]
fn sync_linear_refreshes_every_linked_issue_and_keeps_plans() {
    let board = board_with_two_linked_issues("syncall");

    // An agent's half-ticked plan on PROJ-1 — the state that must survive.
    let plan = "## Spec\n\nThe upstream spec text.\n\n## Plan\n\n\
                ### Task 1: wire it up\n\n\
                - [ ] **Step 1: write the client**\n\
                - [ ] **Step 2: write the tests**\n";
    board
        .run(&["issue", "edit", "PROJ-1", "--description", plan], None)
        .assert_ok();
    board
        .run(&["issue", "tick", "PROJ-1", "--task", "1", "--step", "1"], None)
        .assert_ok();

    let stub = Stub::start(HashMap::from([
        (
            "IssueById:linear-uuid-1".to_string(),
            issue_by_id_reply_for(linear_issue_with(
                "linear-uuid-1",
                "ENG-412",
                "First renamed",
                ("In Progress", "started"),
                "2026-07-30T12:00:00.000Z",
            )),
        ),
        (
            "IssueById:linear-uuid-9".to_string(),
            issue_by_id_reply_for(linear_issue_with(
                "linear-uuid-9",
                "ENG-9",
                "Second renamed",
                ("In Review", "started"),
                "2026-07-30T12:00:00.000Z",
            )),
        ),
    ]));

    let run = board.run(&["sync", "linear", "--json"], Some(&stub));
    run.assert_ok();
    let out: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(out["action"], "sync");
    assert_eq!(out["refreshed"], 2, "{out}");
    assert_eq!(out["skipped"], 0, "{out}");

    let first = board.show("PROJ-1");
    assert_eq!(first["title"], "First renamed");
    assert_eq!(first["status"], "in-progress");
    let description = first["description"].as_str().unwrap();
    assert!(
        description.contains("- [x] **Step 1: write the client**"),
        "the ticked plan must survive a whole-board sync:\n{description}"
    );

    let second = board.show("PROJ-2");
    assert_eq!(second["title"], "Second renamed");
    assert_eq!(second["status"], "in-review");
}

#[test]
fn sync_linear_honors_per_link_origin() {
    let board = Board::new("syncorigin");

    // PROJ-1 is born on the board and pushed out: pushed origin, local spec.
    board
        .run(
            &["issue", "add", "--project", "PROJ", "--title", "Local work"],
            None,
        )
        .assert_ok();
    board
        .run(
            &[
                "issue",
                "edit",
                "PROJ-1",
                "--description",
                "## Spec\n\nThe LOCAL spec, written on the board.\n",
            ],
            None,
        )
        .assert_ok();
    let create_stub = Stub::start(HashMap::from([
        ("TeamByKey".to_string(), team_reply()),
        (
            "IssueCreate".to_string(),
            serde_json::json!({"data": {"issueCreate": {"success": true,
                "issue": linear_issue("Local work", ("Todo", "unstarted"),
                                      "2026-07-29T12:00:00.000Z")}}})
            .to_string(),
        ),
        // The living-comment push path (CLI-43) requires the created comment's
        // id in the reply so it can be edited in place on later pushes.
        (
            "CommentCreate".to_string(),
            comment_create_reply("comment-uuid-sync-1"),
        ),
    ]));
    board
        .run(
            &["push", "linear", "PROJ-1", "--create", "--team", "ENG"],
            Some(&create_stub),
        )
        .assert_ok();

    // PROJ-2 arrives by import: imported origin, Linear's spec.
    let import_stub = Stub::start(import_replies_for(linear_issue_with(
        "linear-uuid-9",
        "ENG-9",
        "Imported work",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    )));
    board
        .run(
            &["import", "linear", "ENG-9", "--project", "PROJ"],
            Some(&import_stub),
        )
        .assert_ok();

    let stub = Stub::start(HashMap::from([
        (
            "IssueById:linear-uuid-1".to_string(),
            issue_by_id_reply_for(linear_issue_with(
                "linear-uuid-1",
                "ENG-412",
                "Local work renamed",
                ("In Progress", "started"),
                "2026-07-30T12:00:00.000Z",
            )),
        ),
        (
            "IssueById:linear-uuid-9".to_string(),
            issue_by_id_reply_for(linear_issue_with(
                "linear-uuid-9",
                "ENG-9",
                "Imported work renamed",
                ("In Progress", "started"),
                "2026-07-30T12:00:00.000Z",
            )),
        ),
    ]));
    board.run(&["sync", "linear"], Some(&stub)).assert_ok();

    let pushed = board.show("PROJ-1");
    assert_eq!(
        pushed["title"], "Local work renamed",
        "Linear owns the title whatever the origin"
    );
    let description = pushed["description"].as_str().unwrap();
    assert!(
        description.contains("The LOCAL spec, written on the board."),
        "a pushed-origin link keeps its board-authored spec through sync:\n{description}"
    );
    assert!(
        !description.contains("The upstream spec text."),
        "sync must not install the upstream spec over a pushed-origin one:\n{description}"
    );

    let imported = board.show("PROJ-2");
    let description = imported["description"].as_str().unwrap();
    assert!(
        description.contains("The upstream spec text."),
        "an imported-origin link refreshes its spec from Linear:\n{description}"
    );
}

#[test]
fn sync_linear_scopes_to_a_project() {
    let board = Board::new("syncproj");
    board
        .run(&["project", "add", "OTHER", "--name", "Elsewhere"], None)
        .assert_ok();

    let stub = Stub::start(import_replies(
        "In scope",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    board
        .run(
            &["import", "linear", "ENG-412", "--project", "PROJ"],
            Some(&stub),
        )
        .assert_ok();
    let stub = Stub::start(import_replies_for(linear_issue_with(
        "linear-uuid-9",
        "ENG-9",
        "Out of scope",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    )));
    board
        .run(
            &["import", "linear", "ENG-9", "--project", "OTHER"],
            Some(&stub),
        )
        .assert_ok();

    // Only PROJ's issue has a reply: touching the other one would error loudly.
    let sync_stub = Stub::start(HashMap::from([(
        "IssueById:linear-uuid-1".to_string(),
        issue_by_id_reply_for(linear_issue_with(
            "linear-uuid-1",
            "ENG-412",
            "In scope renamed",
            ("Todo", "unstarted"),
            "2026-07-30T12:00:00.000Z",
        )),
    )]));
    let run = board.run(
        &["sync", "linear", "--project", "PROJ", "--json"],
        Some(&sync_stub),
    );
    run.assert_ok();
    let out: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(out["refreshed"], 1, "{out}");
    assert_eq!(board.show("PROJ-1")["title"], "In scope renamed");
    assert_eq!(
        board.show("OTHER-1")["title"],
        "Out of scope",
        "--project must fence the sync"
    );
}

#[test]
fn sync_linear_skips_an_issue_gone_upstream() {
    let board = board_with_two_linked_issues("syncgone");
    let stub = Stub::start(HashMap::from([
        (
            // Deleted in Linear: the API returns a null issue.
            "IssueById:linear-uuid-1".to_string(),
            serde_json::json!({"data": {"issue": null}}).to_string(),
        ),
        (
            "IssueById:linear-uuid-9".to_string(),
            issue_by_id_reply_for(linear_issue_with(
                "linear-uuid-9",
                "ENG-9",
                "Still here",
                ("Todo", "unstarted"),
                "2026-07-30T12:00:00.000Z",
            )),
        ),
    ]));
    let run = board.run(&["sync", "linear", "--json"], Some(&stub));
    run.assert_ok();
    let out: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(out["refreshed"], 1, "{out}");
    assert_eq!(out["skipped"], 1, "{out}");
    assert!(
        run.stderr.contains("ENG-412"),
        "the skip should be announced with the remote key: {}",
        run.stderr
    );
    assert_eq!(board.show("PROJ-2")["title"], "Still here");
}

#[test]
fn sync_linear_dry_run_writes_nothing() {
    let board = board_with_two_linked_issues("syncdry");
    let stub = Stub::start(HashMap::from([
        (
            "IssueById:linear-uuid-1".to_string(),
            issue_by_id_reply_for(linear_issue_with(
                "linear-uuid-1",
                "ENG-412",
                "Renamed upstream",
                ("In Progress", "started"),
                "2026-07-30T12:00:00.000Z",
            )),
        ),
        (
            "IssueById:linear-uuid-9".to_string(),
            issue_by_id_reply_for(linear_issue_with(
                "linear-uuid-9",
                "ENG-9",
                "Also renamed",
                ("In Progress", "started"),
                "2026-07-30T12:00:00.000Z",
            )),
        ),
    ]));
    let run = board.run(&["sync", "linear", "--dry-run"], Some(&stub));
    run.assert_ok();
    assert!(run.stdout.contains("dry run"), "{}", run.stdout);
    assert_eq!(
        board.show("PROJ-1")["title"],
        "First issue",
        "dry run must not write"
    );
}

#[test]
fn sync_linear_with_no_links_needs_no_token() {
    let board = Board::new("syncempty");
    // No stub, so no LINEAR_API_KEY: an empty board is knowable without one.
    let run = board.run(&["sync", "linear"], None);
    run.assert_ok();
    assert!(
        run.stdout.contains("nothing linked"),
        "{}",
        run.stdout
    );
}

// ---- import --mine: the inbound queue ----

#[test]
fn import_mine_creates_refreshes_and_skips_out_of_cycle_work() {
    let board = Board::new("mine");

    // ENG-412 is already on the board, so --mine must refresh it, not clone it.
    let stub = Stub::start(import_replies(
        "Original title",
        ("Todo", "unstarted"),
        "2026-07-29T12:00:00.000Z",
    ));
    board
        .run(
            &["import", "linear", "ENG-412", "--project", "PROJ"],
            Some(&stub),
        )
        .assert_ok();

    let mine_stub = Stub::start(HashMap::from([(
        "ViewerAssignedIssues".to_string(),
        viewer_reply(vec![
            // Unlinked, team without cycles: created.
            assigned_node(
                linear_issue_with(
                    "linear-uuid-2",
                    "ENG-2",
                    "Fresh work",
                    ("Todo", "unstarted"),
                    "2026-07-30T12:00:00.000Z",
                ),
                None,
                None,
            ),
            // Already linked: refreshed, with the upstream rename applied.
            assigned_node(
                linear_issue_with(
                    "linear-uuid-1",
                    "ENG-412",
                    "Renamed upstream",
                    ("In Progress", "started"),
                    "2026-07-30T12:00:00.000Z",
                ),
                None,
                None,
            ),
            // The team runs a cycle and this issue is not in it: backlog, skipped.
            assigned_node(
                linear_issue_with(
                    "linear-uuid-3",
                    "ENG-3",
                    "Someday work",
                    ("Todo", "unstarted"),
                    "2026-07-30T12:00:00.000Z",
                ),
                None,
                Some("cyc-active"),
            ),
        ]),
    )]));

    let run = board.run(
        &["import", "linear", "--mine", "--project", "PROJ", "--json"],
        Some(&mine_stub),
    );
    run.assert_ok();
    let out: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(out["action"], "import-mine");
    assert_eq!(out["created"], 1, "{out}");
    assert_eq!(out["refreshed"], 1, "{out}");
    assert_eq!(out["skipped"], 1, "{out}");

    let ls = board.run(&["issue", "ls", "--project", "PROJ", "--json"], None);
    ls.assert_ok();
    let count = ls.stdout.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(count, 2, "created + refreshed, never the skipped one: {}", ls.stdout);

    let issue = board.show("PROJ-1");
    assert_eq!(
        issue["title"], "Renamed upstream",
        "the linked issue should have been refreshed in place"
    );
    assert_eq!(issue["status"], "in-progress");
}

#[test]
fn import_mine_in_an_active_cycle_is_in_scope() {
    let board = Board::new("minecycle");
    let mine_stub = Stub::start(HashMap::from([(
        "ViewerAssignedIssues".to_string(),
        viewer_reply(vec![assigned_node(
            linear_issue_with(
                "linear-uuid-4",
                "ENG-4",
                "This sprint's work",
                ("Todo", "unstarted"),
                "2026-07-30T12:00:00.000Z",
            ),
            Some("cyc-active"),
            Some("cyc-active"),
        )]),
    )]));
    let run = board.run(
        &["import", "linear", "--mine", "--project", "PROJ", "--json"],
        Some(&mine_stub),
    );
    run.assert_ok();
    let out: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(out["created"], 1, "{out}");
    assert_eq!(out["skipped"], 0, "{out}");
}

#[test]
fn import_mine_dry_run_writes_nothing() {
    let board = Board::new("minedry");
    let mine_stub = Stub::start(HashMap::from([(
        "ViewerAssignedIssues".to_string(),
        viewer_reply(vec![assigned_node(
            linear_issue_with(
                "linear-uuid-2",
                "ENG-2",
                "Fresh work",
                ("Todo", "unstarted"),
                "2026-07-30T12:00:00.000Z",
            ),
            None,
            None,
        )]),
    )]));
    let run = board.run(
        &["import", "linear", "--mine", "--project", "PROJ", "--dry-run"],
        Some(&mine_stub),
    );
    run.assert_ok();
    assert!(run.stdout.contains("dry run"), "{}", run.stdout);
    assert!(run.stdout.contains("ENG-2"), "{}", run.stdout);

    let ls = board.run(&["issue", "ls", "--project", "PROJ", "--json"], None);
    ls.assert_ok();
    assert!(
        ls.stdout.trim().is_empty(),
        "dry run created an issue: {}",
        ls.stdout
    );
}

#[test]
fn import_mine_into_a_missing_project_fails_before_writing() {
    let board = Board::new("minenoproj");
    let mine_stub = Stub::start(HashMap::from([(
        "ViewerAssignedIssues".to_string(),
        viewer_reply(vec![assigned_node(
            linear_issue_with(
                "linear-uuid-2",
                "ENG-2",
                "Fresh work",
                ("Todo", "unstarted"),
                "2026-07-30T12:00:00.000Z",
            ),
            None,
            None,
        )]),
    )]));
    let run = board.run(
        &["import", "linear", "--mine", "--project", "NOPE"],
        Some(&mine_stub),
    );
    assert_eq!(run.code, 1, "{run:?}");
    assert!(run.stderr.contains("NOPE"), "{}", run.stderr);
}

#[test]
fn import_linear_needs_exactly_one_of_key_and_mine() {
    let board = Board::new("minearg");
    // Both: clap rejects the conflict before anything runs.
    let run = board.run(
        &["import", "linear", "ENG-412", "--mine", "--project", "PROJ"],
        None,
    );
    assert_ne!(run.code, 0, "{run:?}");
    // Neither: the error should name both ways forward.
    let run = board.run(&["import", "linear", "--project", "PROJ"], None);
    assert_eq!(run.code, 2, "{run:?}");
    assert!(run.stderr.contains("--mine"), "{}", run.stderr);
    // --mine adopts by link, never by --link-to.
    let run = board.run(
        &[
            "import", "linear", "--mine", "--project", "PROJ", "--link-to", "PROJ-1",
        ],
        None,
    );
    assert_ne!(run.code, 0, "{run:?}");
}

// ---- failure paths that need no network at all ----

#[test]
fn push_on_an_unlinked_issue_exits_1_without_needing_a_token() {
    let board = Board::new("unlinked");
    board
        .run(
            &["issue", "add", "--project", "PROJ", "--title", "Local work"],
            None,
        )
        .assert_ok();

    let run = board.run(&["push", "linear", "PROJ-1"], None);
    assert_eq!(run.code, 1, "{run:?}");
    assert!(
        run.stderr.contains("not linked to Linear"),
        "{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("--create"),
        "the message should name the way forward: {}",
        run.stderr
    );
    assert!(
        !run.stderr.contains("LINEAR_API_KEY"),
        "an unlinked issue is knowable without a token: {}",
        run.stderr
    );
}

#[test]
fn push_on_a_missing_issue_exits_1() {
    let board = Board::new("missing");
    let run = board.run(&["push", "linear", "PROJ-999"], None);
    assert_eq!(run.code, 1, "{run:?}");
    assert!(run.stderr.contains("not found"), "{}", run.stderr);
}

#[test]
fn import_with_a_malformed_key_exits_1_before_touching_the_network() {
    let board = Board::new("badkey");
    for bad in ["ENG", "ENG-abc", "412"] {
        let run = board.run(&["import", "linear", bad, "--project", "PROJ"], None);
        assert_eq!(run.code, 1, "{bad:?} should be rejected: {run:?}");
        assert!(
            run.stderr.contains("not a Linear issue key"),
            "{bad:?}: {}",
            run.stderr
        );
    }
}

#[test]
fn import_without_a_token_says_which_variable_to_set() {
    let board = Board::new("notoken");
    // No stub, so no LINEAR_API_KEY in the child environment.
    let run = board.run(&["import", "linear", "ENG-412", "--project", "PROJ"], None);
    assert_eq!(run.code, 2, "{run:?}");
    assert!(run.stderr.contains("LINEAR_API_KEY"), "{}", run.stderr);
    assert!(
        run.stderr.contains("linear.app/settings/api"),
        "point them at where to get one: {}",
        run.stderr
    );
}

#[test]
fn push_create_without_a_team_explains_the_two_ways_to_supply_one() {
    let board = Board::new("noteam");
    board
        .run(
            &["issue", "add", "--project", "PROJ", "--title", "Local work"],
            None,
        )
        .assert_ok();
    let stub = Stub::start(HashMap::new());
    let run = board.run(&["push", "linear", "PROJ-1", "--create"], Some(&stub));
    assert_eq!(run.code, 2, "{run:?}");
    assert!(run.stderr.contains("--team"), "{}", run.stderr);
    assert!(run.stderr.contains("linear.toml"), "{}", run.stderr);
}

impl std::fmt::Debug for Run {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "exit {}\nstdout: {}\nstderr: {}",
            self.code, self.stdout, self.stderr
        )
    }
}
