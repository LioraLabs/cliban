//! Concurrent read-modify-write over the real binary: `issue log`, `issue
//! tick`, `issue edit --section`, and `project note add` under 12 parallel
//! processes.
//!
//! The contract is the one `milestone log` already keeps (CLI-87): every
//! writer exits 0, and every write is present afterward. A deferred
//! transaction fails the first half (`SQLITE_BUSY_SNAPSHOT` is not retried by
//! the busy timeout); no transaction at all fails the second (lost updates).

use std::process::Command;

const WRITERS: usize = 12;

fn bin() -> String {
    env!("CARGO_BIN_EXE_cliban").to_string()
}

fn tmp_db(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("cliban_conc_{tag}_{nanos}.db"));
    path.to_string_lossy().to_string()
}

fn cmd(db: &str) -> Command {
    let mut cmd = Command::new(bin());
    cmd.arg("--db")
        .arg(db)
        .env_remove("CLIBAN_DB")
        .env_remove("XDG_DATA_HOME")
        .env_remove("CLIBAN_ACTOR")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("CLIBAN_OUTPUT")
        .env_remove("CLIBAN_PROJECT");
    cmd
}

fn ok(db: &str, args: &[&str]) -> String {
    let out = cmd(db).args(args).output().expect("run cliban");
    assert_eq!(
        out.status.code().unwrap_or(-1),
        0,
        "`cliban {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// A board with one issue, TST-1, carrying `description`.
fn seeded(tag: &str, description: &str) -> String {
    let db = tmp_db(tag);
    ok(&db, &["project", "add", "TST", "Test"]);
    ok(&db, &["issue", "add", "victim", "-p", "TST"]);
    if !description.is_empty() {
        ok(
            &db,
            &["issue", "edit", "TST-1", "--description", description],
        );
    }
    db
}

fn issue_description(db: &str) -> String {
    ok(db, &["issue", "cat", "TST-1"])
}

/// Run `make(i)` in WRITERS parallel processes; panic listing every nonzero
/// exit.
fn storm(db: &str, make: impl Fn(usize) -> Vec<String> + Sync) {
    let results: Vec<(usize, i32, String)> = std::thread::scope(|s| {
        (0..WRITERS)
            .map(|i| {
                let args = make(i);
                let db = db.to_string();
                s.spawn(move || {
                    let out = cmd(&db).args(&args).output().expect("run cliban");
                    (
                        i,
                        out.status.code().unwrap_or(-1),
                        String::from_utf8_lossy(&out.stderr).to_string(),
                    )
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect()
    });
    let failed: Vec<&(usize, i32, String)> =
        results.iter().filter(|(_, code, _)| *code != 0).collect();
    assert!(
        failed.is_empty(),
        "{} of {WRITERS} concurrent writers were rejected: {failed:?}",
        failed.len()
    );
}

#[test]
fn concurrent_issue_logs_all_survive() {
    let db = seeded("log", "");
    storm(&db, |i| {
        vec![
            "issue".into(),
            "log".into(),
            "TST-1".into(),
            format!("writer {i}"),
        ]
    });
    let desc = issue_description(&db);
    for i in 0..WRITERS {
        assert!(
            desc.contains(&format!(" — writer {i}")),
            "writer {i}'s entry is missing from {desc:?}"
        );
    }
    assert_eq!(
        desc.matches("## Activity Log").count(),
        1,
        "concurrent writers must share one section: {desc:?}"
    );
}

#[test]
fn concurrent_ticks_all_survive() {
    let steps: String = (1..=WRITERS)
        .map(|i| format!("- [ ] **Step {i}: s{i}**\n"))
        .collect();
    let db = seeded(
        "tick",
        &format!("## Spec\n\ns\n\n## Plan\n\n### Task 1: t\n\n{steps}"),
    );
    storm(&db, |i| {
        vec![
            "issue".into(),
            "tick".into(),
            "TST-1".into(),
            "--task".into(),
            "1".into(),
            "--step".into(),
            (i + 1).to_string(),
        ]
    });
    let desc = issue_description(&db);
    assert_eq!(
        desc.matches("- [x]").count(),
        WRITERS,
        "ticks lost: {desc:?}"
    );
}

#[test]
fn concurrent_section_edits_all_accepted() {
    // Same section, different payloads: last write wins on content, but no
    // writer may be turned away — CAS (--if-updated-at) is the opt-in
    // conflict control, rejection is not.
    let db = seeded("edit", "## Spec\n\noriginal\n");
    storm(&db, |i| {
        vec![
            "issue".into(),
            "edit".into(),
            "TST-1".into(),
            "--section".into(),
            "spec".into(),
            "--description".into(),
            format!("spec from writer {i}"),
        ]
    });
    let desc = issue_description(&db);
    assert!(
        desc.contains("spec from writer"),
        "some writer's spec must have landed: {desc:?}"
    );
}

#[test]
fn concurrent_project_notes_all_survive() {
    let db = tmp_db("note");
    ok(&db, &["project", "add", "TST", "Test"]);
    storm(&db, |i| {
        vec![
            "project".into(),
            "note".into(),
            "add".into(),
            "TST".into(),
            format!("note {i}"),
            "--body".into(),
            format!("body {i}"),
        ]
    });
    let notes = ok(&db, &["project", "cat", "TST", "--section", "notes"]);
    for i in 0..WRITERS {
        assert!(
            notes.contains(&format!("### note {i}")),
            "note {i} was lost: {notes:?}"
        );
    }
}

#[test]
fn concurrent_moves_all_accepted() {
    // mv routes through issues::update's own transaction (no outer tx in the
    // command), so this exercises the core-side immediate class: no writer is
    // turned away, whether it changed the status or hit the retry-safe noop.
    let db = seeded("mv", "");
    storm(&db, |i| {
        vec![
            "issue".into(),
            "mv".into(),
            "TST-1".into(),
            if i % 2 == 0 { "in-progress" } else { "backlog" }.into(),
        ]
    });
}
