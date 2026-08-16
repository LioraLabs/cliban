//! Section writes must not change the description's structure beyond the
//! section they target: an unclosed fence in a payload used to swallow every
//! later section, and a note body carrying an H2 used to phantom-split the
//! project description — both with exit 0.

use std::process::Command;

fn bin() -> String {
    env!("CARGO_BIN_EXE_cliban").to_string()
}

fn tmp_db(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("cliban_sect_{tag}_{nanos}.db"));
    path.to_string_lossy().to_string()
}

struct Run {
    stderr: String,
    code: i32,
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

fn run(db: &str, args: &[&str]) -> Run {
    let out = cmd(db).args(args).output().expect("run cliban");
    Run {
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn ok(db: &str, args: &[&str]) -> String {
    let r = cmd(db).args(args).output().expect("run cliban");
    assert_eq!(
        r.status.code().unwrap_or(-1),
        0,
        "`cliban {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&r.stderr)
    );
    String::from_utf8_lossy(&r.stdout).to_string()
}

/// TST-1 with a Spec followed by a Plan — the section an unclosed fence in a
/// Spec payload would swallow.
fn seeded(tag: &str) -> String {
    let db = tmp_db(tag);
    ok(&db, &["project", "add", "TST", "Test"]);
    ok(&db, &["issue", "add", "victim", "-p", "TST"]);
    ok(
        &db,
        &[
            "issue",
            "edit",
            "TST-1",
            "--description",
            "## Spec\n\ns\n\n## Plan\n\n### Task 1: t\n\n- [ ] step\n",
        ],
    );
    db
}

fn plan_intact(db: &str) {
    let plan = ok(db, &["issue", "cat", "TST-1", "--section", "plan"]);
    assert!(plan.contains("Task 1"), "## Plan must survive: {plan:?}");
}

#[test]
fn edit_section_refuses_an_unclosed_fence() {
    let db = seeded("edit");
    let r = run(
        &db,
        &[
            "issue",
            "edit",
            "TST-1",
            "--section",
            "spec",
            "--description",
            "new spec\n\n```\nunclosed",
        ],
    );
    assert_eq!(r.code, 2, "unclosed fence must refuse: {}", r.stderr);
    assert!(r.stderr.contains("fence"), "names the cause: {}", r.stderr);
    plan_intact(&db);
}

#[test]
fn append_section_refuses_an_unclosed_fence() {
    let db = seeded("append");
    let r = run(
        &db,
        &[
            "issue",
            "append-section",
            "TST-1",
            "--section",
            "spec",
            "later\n\n```\nunclosed",
        ],
    );
    assert_eq!(r.code, 2, "unclosed fence must refuse: {}", r.stderr);
    plan_intact(&db);
}

#[test]
fn balanced_fences_and_create_section_still_pass() {
    let db = seeded("pass");
    ok(
        &db,
        &[
            "issue",
            "edit",
            "TST-1",
            "--section",
            "spec",
            "--description",
            "quoting:\n\n```\n## Plan\n```\n\ndone",
        ],
    );
    ok(
        &db,
        &[
            "issue",
            "edit",
            "TST-1",
            "--section",
            "Rollout",
            "--create-section",
            "--description",
            "a new custom section",
        ],
    );
    plan_intact(&db);
}

#[test]
fn note_add_refuses_a_structure_breaking_body() {
    let db = tmp_db("note");
    ok(&db, &["project", "add", "TST", "Test"]);
    ok(
        &db,
        &[
            "project",
            "edit",
            "TST",
            "--description",
            "## Notes\n\n### old lesson\n\nkeep\n\n## Roadmap\n\nfuture\n",
        ],
    );
    let fence = run(
        &db,
        &["project", "note", "add", "TST", "bad", "--body", "x\n\n```\nunclosed"],
    );
    assert_eq!(fence.code, 2, "fence body must refuse: {}", fence.stderr);
    let h2 = run(
        &db,
        &["project", "note", "add", "TST", "worse", "--body", "x\n\n## Phantom\n\ny"],
    );
    assert_eq!(h2.code, 2, "H2 body must refuse: {}", h2.stderr);
    let desc = ok(&db, &["project", "cat", "TST"]);
    assert!(desc.contains("future"), "## Roadmap must survive: {desc:?}");
}
