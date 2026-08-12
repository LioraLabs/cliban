//! Retry-safe mutations: desired state = success.
//!
//! An agent retrying a mutation after a timeout is asking for a state, not for
//! an event. When the board already IS that state — the step is checked, the
//! issue sits in that column, the archived bit already matches — the command
//! exits 0 with an explicit "nothing to do" note (table) or a `"noop": true`
//! field on the usual JSON echo. Genuinely wrong targets stay hard errors with
//! their long-standing codes — no such task/step is exit 2, an unparseable
//! status exit 3, a missing key exit 1: mistakes, not achieved intents.
//!
//! No PTY here (same as output_contract.rs): table mode is exercised via
//! `CLIBAN_OUTPUT=table`, json via the piped default.

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
        .join(format!("cliban_retrysafe_{tag}_{nanos}.db"))
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

fn run(db: &str, args: &[&str]) -> Run {
    run_env(db, args, &[])
}

fn ok(db: &str, args: &[&str]) -> String {
    let r = run(db, args);
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

const TABLE: &[(&str, &str)] = &[("CLIBAN_OUTPUT", "table")];

/// A board with one planned issue (CLI-1) and one plain issue (CLI-2).
fn seeded(tag: &str) -> String {
    let db = tmp_db(tag);
    ok(&db, &["project", "add", "CLI", "Cliban"]);
    ok(
        &db,
        &[
            "issue",
            "add",
            "planned",
            "--project",
            "CLI",
            "--description",
            "## Plan\n\n### Task 1: t\n\n- [ ] Step 1\n- [ ] Step 2\n",
        ],
    );
    ok(&db, &["issue", "add", "plain", "--project", "CLI"]);
    db
}

/// Number of NDJSON activity records on the board — noop mutations must not
/// add audit spam.
fn activity_count(db: &str) -> usize {
    ok(db, &["activity", "--since", "1d", "--json"])
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

// --- tick --------------------------------------------------------------------

#[test]
fn retick_of_a_checked_step_succeeds_and_says_nothing_to_do() {
    let db = seeded("retick");
    let first = ok(
        &db,
        &["issue", "tick", "CLI-1", "--task", "1", "--step", "1"],
    );
    let v: serde_json::Value = serde_json::from_str(&first).expect("tick echoes JSON");
    assert_eq!(v["checked"], true);
    assert!(
        v.get("noop").is_none(),
        "a real tick carries no noop marker: {first}"
    );

    let desc_before = ok(&db, &["issue", "cat", "CLI-1", "--section", "plan"]);
    let audits_before = activity_count(&db);

    // Table mode: exit 0 with the explicit note.
    let confirm = ok_env(
        &db,
        &["issue", "tick", "CLI-1", "--task", "1", "--step", "1"],
        TABLE,
    );
    assert_eq!(
        confirm,
        "ticked CLI-1 Task 1 Step 1 (already checked — nothing to do)\n"
    );

    // JSON mode: same echo shape plus "noop": true, updated_at untouched.
    let show: serde_json::Value =
        serde_json::from_str(&ok(&db, &["issue", "show", "CLI-1", "--json"])).unwrap();
    let echo = ok(
        &db,
        &["issue", "tick", "CLI-1", "--task", "1", "--step", "1"],
    );
    let v: serde_json::Value = serde_json::from_str(&echo).expect("noop tick echoes JSON");
    assert_eq!(v["noop"], true);
    assert_eq!(v["checked"], true);
    assert_eq!(v["key"], "CLI-1");
    assert_eq!(v["task"], 1);
    assert_eq!(v["step"], 1);
    assert_eq!(
        v["updated_at"], show["updated_at"],
        "a noop tick must not touch updated_at"
    );

    // The plan text and the audit trail are exactly as they were.
    assert_eq!(
        ok(&db, &["issue", "cat", "CLI-1", "--section", "plan"]),
        desc_before
    );
    assert_eq!(
        activity_count(&db),
        audits_before,
        "noop ticks must not add audit records"
    );
}

#[test]
fn tick_of_wrong_targets_stays_exit_2() {
    let db = seeded("tickwrong");
    for args in [
        ["issue", "tick", "CLI-1", "--task", "1", "--step", "9"],
        ["issue", "tick", "CLI-1", "--task", "9", "--step", "1"],
        // The second fixture issue has no ## Plan at all.
        ["issue", "tick", "CLI-2", "--task", "1", "--step", "1"],
    ] {
        let r = run(&db, &args);
        assert_eq!(r.code, 2, "`cliban {}`: {}", args.join(" "), r.stderr);
    }
    // A missing key is not-found, not validation.
    let r = run(
        &db,
        &["issue", "tick", "CLI-99", "--task", "1", "--step", "1"],
    );
    assert_eq!(r.code, 1, "{}", r.stderr);
}

// --- mv ----------------------------------------------------------------------

#[test]
fn mv_to_the_current_status_succeeds_and_says_already_there() {
    let db = seeded("remv");
    ok(&db, &["issue", "mv", "CLI-1", "in-progress"]);

    let before = ok(&db, &["issue", "show", "CLI-1", "--json"]);
    let audits_before = activity_count(&db);

    // Table mode: exit 0, names the entity and the (already-held) status.
    let confirm = ok_env(&db, &["issue", "mv", "CLI-1", "in-progress"], TABLE);
    assert_eq!(confirm, "CLI-1 already in-progress (nothing to do)\n");

    // JSON mode: the usual full-issue echo, plus "noop": true.
    let echo = ok(&db, &["issue", "mv", "CLI-1", "in-progress"]);
    let v: serde_json::Value = serde_json::from_str(&echo).expect("noop mv echoes JSON");
    assert_eq!(v["noop"], true);
    assert_eq!(v["key"], "CLI-1");
    assert_eq!(v["status"], "in-progress");

    // No reposition, no updated_at churn, no audit spam.
    assert_eq!(ok(&db, &["issue", "show", "CLI-1", "--json"]), before);
    assert_eq!(
        activity_count(&db),
        audits_before,
        "noop mvs must not add audit records"
    );
}

#[test]
fn mv_still_moves_and_wrong_status_stays_exit_2() {
    let db = seeded("mvwrong");
    // A real move still confirms the transition.
    let confirm = ok_env(&db, &["issue", "mv", "CLI-1", "in-progress"], TABLE);
    assert_eq!(confirm, "moved CLI-1: backlog → in-progress\n");
    // Garbage status is a mistake, not an intent. (Exit 3 is this CLI's
    // long-standing Go-parity code for an unparseable status — the point here
    // is that it stays a hard error, not that it becomes a noop success.)
    let r = run(&db, &["issue", "mv", "CLI-1", "sideways"]);
    assert_eq!(r.code, 3, "{}", r.stderr);
    // Missing key stays not-found.
    let r = run(&db, &["issue", "mv", "CLI-99", "backlog"]);
    assert_eq!(r.code, 1, "{}", r.stderr);
}

// --- archive / unarchive -----------------------------------------------------

#[test]
fn archive_of_an_archived_issue_succeeds_noting_the_noop() {
    let db = seeded("rearch");
    ok(&db, &["issue", "archive", "CLI-2"]);

    let before = ok(&db, &["issue", "show", "CLI-2", "--json"]);
    let audits_before = activity_count(&db);

    let confirm = ok_env(&db, &["issue", "archive", "CLI-2"], TABLE);
    assert_eq!(confirm, "CLI-2 already archived (nothing to do)\n");

    let echo = ok(&db, &["issue", "archive", "CLI-2"]);
    let v: serde_json::Value = serde_json::from_str(&echo).unwrap();
    assert_eq!(v["noop"], true);
    assert_eq!(v["archived"], true);

    assert_eq!(ok(&db, &["issue", "show", "CLI-2", "--json"]), before);
    assert_eq!(activity_count(&db), audits_before);
}

#[test]
fn unarchive_of_an_unarchived_issue_succeeds_noting_the_noop() {
    let db = seeded("reunarch");
    let audits_before = activity_count(&db);

    let confirm = ok_env(&db, &["issue", "unarchive", "CLI-2"], TABLE);
    assert_eq!(confirm, "CLI-2 already unarchived (nothing to do)\n");

    let echo = ok(&db, &["issue", "unarchive", "CLI-2"]);
    let v: serde_json::Value = serde_json::from_str(&echo).unwrap();
    assert_eq!(v["noop"], true);
    assert!(v.get("archived").is_none(), "absent means unarchived: {v}");
    assert_eq!(activity_count(&db), audits_before);

    // The real flips still confirm without a noop marker.
    let echo = ok(&db, &["issue", "archive", "CLI-2"]);
    let v: serde_json::Value = serde_json::from_str(&echo).unwrap();
    assert!(v.get("noop").is_none(), "{echo}");
    let confirm = ok_env(&db, &["issue", "unarchive", "CLI-2"], TABLE);
    assert_eq!(confirm, "unarchived CLI-2\n");
}
