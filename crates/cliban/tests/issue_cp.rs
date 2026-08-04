//! `issue cp`: duplicate the shape, never the history.
//!
//! The copy gets the title (unless overridden), `## Spec`, `## Plan` with
//! every checkbox reset, `## Notes`, labels, priority, and — same project
//! only — the milestone. It never gets the activity log, claims, relations,
//! due date, completed_at, or archived state, and it always starts in
//! backlog. The source is left byte-identical.

use std::process::Command;

fn bin() -> String {
    env!("CARGO_BIN_EXE_cliban").to_string()
}

fn tmp_db(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("cliban_issuecp_{tag}_{nanos}.db"))
        .to_string_lossy()
        .to_string()
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Spawn the binary with piped stdout and a scrubbed env, plus `extra_env`.
fn run_env(db: &str, args: &[&str], extra_env: &[(&str, &str)]) -> Run {
    let mut cmd = Command::new(bin());
    cmd.arg("--db")
        .arg(db)
        .env_remove("CLIBAN_DB")
        .env_remove("XDG_DATA_HOME")
        .env_remove("CLIBAN_ACTOR")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("CLIBAN_OUTPUT")
        .env_remove("CLIBAN_PROJECT")
        .args(args);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run cliban");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn ok(db: &str, args: &[&str]) -> String {
    let r = run_env(db, args, &[]);
    assert_eq!(
        r.code,
        0,
        "`cliban {}` failed: {}",
        args.join(" "),
        r.stderr
    );
    r.stdout
}

fn ok_env(db: &str, args: &[&str], extra_env: &[(&str, &str)]) -> String {
    let r = run_env(db, args, extra_env);
    assert_eq!(
        r.code,
        0,
        "`cliban {}` failed: {}",
        args.join(" "),
        r.stderr
    );
    r.stdout
}

fn show_json(db: &str, key: &str) -> serde_json::Value {
    serde_json::from_str(&ok(db, &["issue", "show", key, "--json"])).expect("issue json")
}

const SOURCE_DESC: &str = "## Spec\n\nthe spec body\n\n## Plan\n\n### Task 1: build\n\n\
- [x] **Step 1: done step** → CLI-9\n- [ ] **Step 2: open step**\n  - [x] child bullet stays\n\n\
## Decisions so far\n\n- instance deliberation\n\n## Notes\n\ndurable lesson\n";

/// A board with a rich source issue CLI-1: full description contract, label,
/// priority, milestone, due date, a relation, and a live claim.
fn seeded(tag: &str) -> String {
    let db = tmp_db(tag);
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(&db, &["milestone", "add", "v1", "--project", "CLI"]);
    ok(&db, &["label", "add", "feature", "--project", "CLI"]);
    ok(
        &db,
        &[
            "issue",
            "add",
            "template issue",
            "--project",
            "CLI",
            "--description",
            SOURCE_DESC,
            "--priority",
            "high",
            "--milestone",
            "v1",
            "--due",
            "2026-01-15",
            "--label",
            "feature",
        ],
    );
    // A second issue so the source can carry a relation.
    ok(&db, &["issue", "add", "other", "--project", "CLI"]);
    ok(&db, &["issue", "edit", "CLI-1", "--blocks", "CLI-2"]);
    ok(&db, &["issue", "claim", "CLI-1", "--by", "someone"]);
    // History on the source that must never travel.
    ok(&db, &["issue", "log", "CLI-1", "original history entry"]);
    db
}

#[test]
fn cp_copies_the_shape_and_never_the_history() {
    let db = seeded("shape");
    let before = show_json(&db, "CLI-1");

    let out = ok(&db, &["issue", "cp", "CLI-1", "--json"]);
    let copy: serde_json::Value = serde_json::from_str(&out).expect("cp --json echoes the issue");

    // Shape: title, priority, labels, milestone (same project).
    assert_eq!(copy["key"], "CLI-3");
    assert_eq!(copy["title"], "template issue");
    assert_eq!(copy["priority"], "high");
    assert_eq!(copy["labels"], serde_json::json!(["feature"]));
    assert_eq!(copy["milestone"], "v1");
    assert_eq!(copy["status"], "backlog");
    assert_eq!(copy["archived"], false);

    // Description: Spec + reset Plan + Notes, nothing else.
    let desc = copy["description"].as_str().unwrap();
    assert!(desc.contains("## Spec\n\nthe spec body\n"), "{desc:?}");
    assert!(
        desc.contains("- [ ] **Step 1: done step**\n"),
        "checkbox reset + promotion suffix stripped: {desc:?}"
    );
    assert!(desc.contains("- [ ] **Step 2: open step**"));
    assert!(
        desc.contains("  - [x] child bullet stays"),
        "indented boxes are not steps: {desc:?}"
    );
    assert!(desc.contains("## Notes\n\ndurable lesson\n"));
    assert!(!desc.contains("Activity Log"), "{desc:?}");
    assert!(!desc.contains("original history entry"));
    assert!(!desc.contains("Decisions so far"));
    assert!(
        !desc.contains("CLI-9"),
        "dangling promotion pointer: {desc:?}"
    );

    // History: never copied.
    assert_eq!(copy["relations"], serde_json::json!([]));
    assert!(copy.get("claimed_by").is_none(), "copy must be unclaimed");
    assert_eq!(copy["due_date"], serde_json::Value::Null);
    assert_eq!(copy["completed_at"], serde_json::Value::Null);

    // The source is untouched (only updated_at-invariant fields compared).
    let after = show_json(&db, "CLI-1");
    assert_eq!(before["description"], after["description"]);
    assert_eq!(before["status"], after["status"]);
    assert_eq!(before["claimed_by"], after["claimed_by"]);
    assert_eq!(before["relations"], after["relations"]);
    assert_eq!(before["due_date"], after["due_date"]);
}

#[test]
fn cp_title_overrides_and_table_mode_confirms() {
    let db = seeded("title");
    let out = ok_env(
        &db,
        &["issue", "cp", "CLI-1", "--title", "second run"],
        &[("CLIBAN_OUTPUT", "table")],
    );
    assert_eq!(out, "copied CLI-3: second run\n");
    let copy = show_json(&db, "CLI-3");
    assert_eq!(copy["title"], "second run");
    // Piped default is JSON: no flags, no env → entity echo.
    let piped = ok(&db, &["issue", "cp", "CLI-1"]);
    let v: serde_json::Value = serde_json::from_str(&piped).expect("piped cp echoes JSON");
    assert_eq!(v["key"], "CLI-4");
}

#[test]
fn cp_cross_project_drops_the_milestone() {
    let db = seeded("xproj");
    ok(&db, &["project", "add", "OPS", "--name", "Ops"]);
    let out = ok(&db, &["issue", "cp", "CLI-1", "--project", "OPS", "--json"]);
    let copy: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(copy["key"], "OPS-1");
    assert_eq!(
        copy["milestone"],
        serde_json::Value::Null,
        "milestone names are project-scoped; cross-project cp drops it"
    );
    assert_eq!(copy["labels"], serde_json::json!(["feature"]));
    assert_eq!(copy["priority"], "high");
}

#[test]
fn cp_records_provenance_on_the_copy() {
    let db = seeded("audit");
    ok(&db, &["issue", "cp", "CLI-1", "--json"]);
    let activity = ok(&db, &["activity", "--issue", "CLI-3", "--table"]);
    assert!(
        activity.contains("copied from CLI-1"),
        "audit entry missing: {activity:?}"
    );
    // Cross-project: the drop reason is part of the record.
    ok(&db, &["project", "add", "OPS", "--name", "Ops"]);
    ok(&db, &["issue", "cp", "CLI-1", "--project", "OPS", "--json"]);
    let activity = ok(&db, &["activity", "--issue", "OPS-1", "--table"]);
    assert!(
        activity.contains("copied from CLI-1")
            && activity.contains("milestone")
            && activity.contains("dropped"),
        "cross-project audit entry should name the milestone drop: {activity:?}"
    );
}

#[test]
fn cp_of_a_missing_issue_is_not_found() {
    let db = tmp_db("missing");
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    let r = run_env(&db, &["issue", "cp", "CLI-99", "--json"], &[]);
    assert_ne!(r.code, 0);
    assert!(r.stderr.contains("not found"), "{}", r.stderr);
}
