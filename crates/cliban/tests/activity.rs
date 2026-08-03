//! `cliban activity` end-to-end: the feed an agent reads to answer "what
//! happened since yesterday?".

use std::process::Command;

fn bin() -> String {
    env!("CARGO_BIN_EXE_cliban").to_string()
}

fn tmp_db(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("cliban_activity_{tag}_{nanos}.db"));
    path.to_string_lossy().to_string()
}

struct Run {
    stdout: String,
    code: i32,
}

fn run(db: &str, args: &[&str]) -> Run {
    run_as(db, None, args)
}

/// Run with an optional `$CLIBAN_ACTOR`.
fn run_as(db: &str, actor: Option<&str>, args: &[&str]) -> Run {
    let mut cmd = Command::new(bin());
    cmd.arg("--db")
        .arg(db)
        // A clean env: never inherit the developer's CLIBAN_DB — nor the
        // ambient Claude session, which would auto-attribute every entry.
        .env_remove("CLIBAN_DB")
        .env_remove("XDG_DATA_HOME")
        .env_remove("CLIBAN_ACTOR")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .args(args);
    if let Some(a) = actor {
        cmd.env("CLIBAN_ACTOR", a);
    }
    let out = cmd.output().expect("run cliban");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn ok_as(db: &str, actor: &str, args: &[&str]) -> String {
    let r = run_as(db, Some(actor), args);
    assert_eq!(r.code, 0, "`cliban {}` failed", args.join(" "));
    r.stdout
}

fn ok(db: &str, args: &[&str]) -> String {
    let r = run(db, args);
    assert_eq!(r.code, 0, "`cliban {}` failed", args.join(" "));
    r.stdout
}

/// A board with one created issue, one moved to done, and one logged note.
fn seeded() -> String {
    let db = tmp_db("feed");
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "alpha"],
    );
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "beta"],
    );
    ok(&db, &["issue", "mv", "CLI-2", "done"]);
    ok(&db, &["issue", "log", "CLI-1", "rebased onto main"]);
    db
}

fn events(db: &str, extra: &[&str]) -> Vec<serde_json::Value> {
    let mut args = vec!["activity", "--json"];
    args.extend_from_slice(extra);
    ok(db, &args)
        .lines()
        .map(|l| serde_json::from_str(l).expect("valid NDJSON"))
        .collect()
}

fn kinds(evs: &[serde_json::Value]) -> Vec<(String, String)> {
    evs.iter()
        .map(|e| {
            (
                e["key"].as_str().unwrap().to_string(),
                e["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn feed_reports_created_completed_and_logged_events_newest_first() {
    let db = seeded();
    let evs = events(&db, &[]);
    let k = kinds(&evs);
    assert!(
        k.contains(&("CLI-1".into(), "created".into())),
        "created event missing: {k:?}"
    );
    assert!(
        k.contains(&("CLI-2".into(), "completed".into())),
        "completed beats a bare update: {k:?}"
    );
    assert!(
        k.contains(&("CLI-1".into(), "log".into())),
        "log line missing: {k:?}"
    );
    // `updated` is only a fallback: an issue already reported as created (or
    // completed) must not also show up as a bare update.
    assert!(
        !k.iter().any(|(_, kind)| kind == "updated"),
        "no redundant `updated` alongside created/completed: {k:?}"
    );

    // Newest first.
    let ts: Vec<&str> = evs.iter().map(|e| e["ts"].as_str().unwrap()).collect();
    let mut sorted = ts.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(ts, sorted, "feed must be newest-first");

    // The log event carries its prose; state events carry null.
    let log = evs
        .iter()
        .find(|e| e["kind"] == "log")
        .expect("a log event");
    assert_eq!(log["message"], "rebased onto main");
    assert_eq!(log["title"], "alpha");
    let created = evs.iter().find(|e| e["kind"] == "created").unwrap();
    assert!(created["message"].is_null());
}

#[test]
fn since_accepts_the_forms_agents_actually_type() {
    let db = seeded();
    for form in ["1d", "3d", "2w", "today", "yesterday", "4h", "90m"] {
        let r = run(&db, &["activity", "--since", form, "--json"]);
        assert_eq!(r.code, 0, "--since {form} should be accepted");
        assert!(!r.stdout.is_empty(), "--since {form} found nothing");
    }
    // Same vocabulary on issue ls.
    for form in ["1d", "yesterday", "2026-01-01"] {
        let r = run(&db, &["issue", "ls", "--updated-since", form, "--json"]);
        assert_eq!(r.code, 0, "--updated-since {form} should be accepted");
    }
}

#[test]
fn a_bad_since_explains_the_accepted_forms() {
    let db = seeded();
    let out = Command::new(bin())
        .args(["--db", &db, "activity", "--since", "last tuesday"])
        .env_remove("CLIBAN_DB")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("3d") && err.contains("yesterday"), "{err}");
}

#[test]
fn an_old_window_reports_nothing_rather_than_everything() {
    let db = seeded();
    let evs = events(&db, &["--since", "2020-01-01"]);
    assert!(!evs.is_empty(), "a wide window sees the seeded work");

    // A window that starts in the future excludes everything: empty NDJSON,
    // and a plain-language line in the human form.
    assert!(events(&db, &["--since", "2099-01-01"]).is_empty());
    let text = ok(&db, &["activity", "--since", "2099-01-01"]);
    assert!(text.contains("no activity since"), "{text}");
}

#[test]
fn an_issue_created_and_finished_in_the_window_reports_both() {
    let db = tmp_db("both");
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "quick"],
    );
    ok(&db, &["issue", "mv", "CLI-1", "done"]);
    let k = kinds(&events(&db, &[]));
    assert!(k.contains(&("CLI-1".into(), "created".into())), "{k:?}");
    assert!(k.contains(&("CLI-1".into(), "completed".into())), "{k:?}");
}

// --- what cliban records on its own -----------------------------------------

#[test]
fn a_move_records_its_transition_without_being_asked() {
    let db = tmp_db("auto");
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "work"],
    );
    ok(&db, &["issue", "mv", "CLI-1", "in-progress"]);
    ok(&db, &["issue", "mv", "CLI-1", "in-review"]);

    let msgs: Vec<String> = events(&db, &[])
        .iter()
        .filter(|e| e["kind"] == "status")
        .map(|e| e["message"].as_str().unwrap().to_string())
        .collect();
    assert!(
        msgs.contains(&"backlog → in-progress".to_string()),
        "{msgs:?}"
    );
    assert!(
        msgs.contains(&"in-progress → in-review".to_string()),
        "{msgs:?}"
    );
}

#[test]
fn a_note_records_the_why_alongside_the_transition() {
    let db = tmp_db("note");
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "work"],
    );
    ok(
        &db,
        &[
            "issue",
            "mv",
            "CLI-1",
            "blocked",
            "--note",
            "waiting on upstream fix",
        ],
    );
    let e = events(&db, &[]);
    let status = e.iter().find(|e| e["kind"] == "status").expect("recorded");
    assert_eq!(
        status["message"],
        "backlog → blocked: waiting on upstream fix"
    );
}

#[test]
fn cliban_actor_attributes_recorded_entries() {
    let db = tmp_db("actor");
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "work"],
    );
    ok_as(&db, "claude", &["issue", "mv", "CLI-1", "in-progress"]);
    ok_as(&db, "alex", &["issue", "mv", "CLI-1", "done"]);

    let actors: Vec<String> = events(&db, &[])
        .iter()
        .filter(|e| e["kind"] == "status")
        .filter_map(|e| e["actor"].as_str().map(str::to_string))
        .collect();
    assert!(actors.contains(&"claude".to_string()), "{actors:?}");
    assert!(actors.contains(&"alex".to_string()), "{actors:?}");

    // Unset actor records the transition anyway, just unattributed.
    ok(&db, &["issue", "mv", "CLI-1", "backlog"]);
    let unattributed = events(&db, &[])
        .into_iter()
        .filter(|e| e["kind"] == "status")
        .any(|e| e["actor"].is_null());
    assert!(unattributed, "a move without an actor is still recorded");
}

#[test]
fn ambient_claude_session_attributes_when_no_actor_is_set() {
    let db = tmp_db("session_actor");
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "work"],
    );

    // No CLIBAN_ACTOR, but a Claude Code session in the environment: the
    // session id becomes the actor, shortened for the timeline.
    let mut cmd = Command::new(bin());
    cmd.arg("--db")
        .arg(&db)
        .env_remove("CLIBAN_DB")
        .env_remove("XDG_DATA_HOME")
        .env_remove("CLIBAN_ACTOR")
        .env(
            "CLAUDE_CODE_SESSION_ID",
            "ea8a9c5e-a7b9-4f56-8bda-37c4b8abb59b",
        )
        .args(["issue", "mv", "CLI-1", "in-progress"]);
    assert!(cmd.output().expect("run").status.success());

    // An explicit CLIBAN_ACTOR still wins over the ambient session.
    let mut cmd = Command::new(bin());
    cmd.arg("--db")
        .arg(&db)
        .env_remove("CLIBAN_DB")
        .env_remove("XDG_DATA_HOME")
        .env("CLIBAN_ACTOR", "alex")
        .env(
            "CLAUDE_CODE_SESSION_ID",
            "ea8a9c5e-a7b9-4f56-8bda-37c4b8abb59b",
        )
        .args(["issue", "mv", "CLI-1", "done"]);
    assert!(cmd.output().expect("run").status.success());

    let actors: Vec<String> = events(&db, &[])
        .iter()
        .filter(|e| e["kind"] == "status")
        .filter_map(|e| e["actor"].as_str().map(str::to_string))
        .collect();
    assert!(actors.contains(&"session:ea8a9c5e".to_string()), "{actors:?}");
    assert!(actors.contains(&"alex".to_string()), "{actors:?}");
}

#[test]
fn archiving_is_recorded_too() {
    let db = tmp_db("arch");
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "work"],
    );
    ok(&db, &["issue", "archive", "CLI-1"]);
    ok(&db, &["issue", "unarchive", "CLI-1"]);
    let msgs: Vec<String> = events(&db, &["--archived"])
        .iter()
        .filter(|e| e["kind"] == "archive")
        .map(|e| e["message"].as_str().unwrap().to_string())
        .collect();
    assert!(msgs.contains(&"archived".to_string()), "{msgs:?}");
    assert!(msgs.contains(&"unarchived".to_string()), "{msgs:?}");
}

#[test]
fn section_activity_merges_written_prose_with_recorded_transitions() {
    let db = tmp_db("merge");
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "work"],
    );

    // Nothing recorded yet and no section written: still a clean 1.
    assert_eq!(
        run(&db, &["issue", "show", "CLI-1", "--section", "activity"]).code,
        1
    );

    ok_as(&db, "claude", &["issue", "log", "CLI-1", "starting now"]);
    // A logged note is recorded as well as written to the markdown, so it
    // appears exactly once and carries its author.
    let written = ok(&db, &["issue", "show", "CLI-1", "--section", "activity"]);
    assert_eq!(written.matches("starting now").count(), 1, "{written}");
    assert!(written.contains("[claude] starting now"), "{written}");

    ok_as(&db, "claude", &["issue", "mv", "CLI-1", "done"]);
    let merged = ok(&db, &["issue", "show", "CLI-1", "--section", "activity"]);
    assert!(merged.contains("starting now"), "keeps the prose: {merged}");
    assert!(
        merged.contains("[claude] backlog → done"),
        "adds the recorded transition: {merged}"
    );
    // One chronological list, oldest first.
    let prose_at = merged.find("starting now").unwrap();
    let move_at = merged.find("backlog → done").unwrap();
    assert!(prose_at < move_at, "chronological order: {merged}");
}

/// Messages of the recorded entries of one kind, for one issue.
fn recorded(db: &str, kind: &str) -> Vec<String> {
    events(db, &["--since", "1d"])
        .iter()
        .filter(|e| e["kind"] == kind)
        .map(|e| e["message"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn one_issue(tag: &str) -> String {
    let db = tmp_db(tag);
    ok(&db, &["project", "add", "CLI", "--name", "Cliban"]);
    ok(
        &db,
        &[
            "issue",
            "add",
            "--project",
            "CLI",
            "--title",
            "orig",
            "--priority",
            "low",
        ],
    );
    db
}

#[test]
fn an_edit_records_which_fields_moved() {
    let db = one_issue("edit");
    ok_as(
        &db,
        "claude",
        &[
            "issue",
            "edit",
            "CLI-1",
            "--title",
            "renamed",
            "--priority",
            "urgent",
        ],
    );
    let msgs = recorded(&db, "edit");
    assert_eq!(msgs.len(), 1, "one entry per edit invocation: {msgs:?}");
    assert!(msgs[0].contains("title: orig → renamed"), "{msgs:?}");
    assert!(msgs[0].contains("priority: low → urgent"), "{msgs:?}");

    let actor = events(&db, &[])
        .into_iter()
        .find(|e| e["kind"] == "edit")
        .map(|e| e["actor"].as_str().unwrap_or_default().to_string());
    assert_eq!(actor.as_deref(), Some("claude"));
}

#[test]
fn an_edit_that_changes_nothing_records_nothing() {
    let db = one_issue("noop");
    // Setting a field to the value it already holds is not an event.
    ok(&db, &["issue", "edit", "CLI-1", "--priority", "low"]);
    assert!(recorded(&db, "edit").is_empty());
}

#[test]
fn label_and_relation_changes_are_recorded() {
    let db = one_issue("labels");
    ok(
        &db,
        &["issue", "add", "--project", "CLI", "--title", "other"],
    );
    ok(&db, &["issue", "edit", "CLI-1", "--label", "bug"]);
    ok(&db, &["issue", "edit", "CLI-1", "--remove-label", "bug"]);
    ok(&db, &["issue", "edit", "CLI-1", "--blocked-by", "CLI-2"]);
    ok(
        &db,
        &["issue", "edit", "CLI-1", "--remove-relation", "CLI-2"],
    );
    let msgs = recorded(&db, "edit");
    assert!(msgs.contains(&"+label bug".to_string()), "{msgs:?}");
    assert!(msgs.contains(&"-label bug".to_string()), "{msgs:?}");
    assert!(msgs.contains(&"+blocked-by CLI-2".to_string()), "{msgs:?}");
    assert!(msgs.contains(&"-relation CLI-2".to_string()), "{msgs:?}");
}

#[test]
fn a_description_rewrite_names_the_sections_it_destroyed() {
    let db = one_issue("wipe");
    ok(&db, &["issue", "log", "CLI-1", "a note worth keeping"]);
    let plan = std::env::temp_dir().join("cliban_test_plan.md");
    std::fs::write(&plan, "## Plan\n\n### Task 1: t\n\n- [ ] **Step 1: a**\n").unwrap();
    ok(
        &db,
        &[
            "issue",
            "edit",
            "CLI-1",
            "--description-file",
            plan.to_str().unwrap(),
        ],
    );
    let msgs = recorded(&db, "edit");
    assert!(
        msgs.iter().any(|m| m.contains("dropped ## Activity Log")),
        "the rewrite silently wiped a section; the timeline must say so: {msgs:?}"
    );
    // …and the note itself survives, because `issue log` records durably as
    // well as writing markdown.
    let logs = recorded(&db, "log");
    assert!(
        logs.contains(&"a note worth keeping".to_string()),
        "a logged note must outlive a description rewrite: {logs:?}"
    );
}

#[test]
fn a_logged_note_is_not_reported_twice() {
    let db = one_issue("dedupe");
    ok(&db, &["issue", "log", "CLI-1", "exactly once please"]);
    let logs = recorded(&db, "log");
    assert_eq!(
        logs.iter().filter(|m| *m == "exactly once please").count(),
        1,
        "written to markdown *and* recorded, but shown once: {logs:?}"
    );
    // Same in the per-issue view.
    let view = ok(&db, &["issue", "show", "CLI-1", "--section", "activity"]);
    assert_eq!(view.matches("exactly once please").count(), 1, "{view}");
}

#[test]
fn plan_ticks_and_promotions_are_recorded_on_both_issues() {
    let db = one_issue("plan");
    let plan = std::env::temp_dir().join("cliban_test_plan2.md");
    std::fs::write(
        &plan,
        "## Plan\n\n### Task 1: t\n\n- [ ] **Step 1: a**\n- [ ] **Step 2: b**\n",
    )
    .unwrap();
    ok(
        &db,
        &[
            "issue",
            "edit",
            "CLI-1",
            "--description-file",
            plan.to_str().unwrap(),
        ],
    );
    ok(
        &db,
        &["issue", "tick", "CLI-1", "--task", "1", "--step", "1"],
    );
    ok(
        &db,
        &[
            "issue", "promote", "CLI-1", "--task", "1", "--step", "2", "--title", "split",
        ],
    );
    let msgs = recorded(&db, "plan");
    assert!(
        msgs.contains(&"ticked Task 1 Step 1".to_string()),
        "{msgs:?}"
    );
    assert!(
        msgs.contains(&"promoted Task 1 Step 2 → CLI-2".to_string()),
        "the parent records what it lost: {msgs:?}"
    );
    assert!(
        msgs.contains(&"promoted from CLI-1 Task 1 Step 2".to_string()),
        "the new issue records where it came from: {msgs:?}"
    );
}

#[test]
fn filters_narrow_the_feed() {
    let db = seeded();
    ok(&db, &["project", "add", "LM", "--name", "Loom"]);
    ok(
        &db,
        &["issue", "add", "--project", "LM", "--title", "other"],
    );

    let cli_only = events(&db, &["--project", "CLI"]);
    assert!(
        cli_only.iter().all(|e| e["project"] == "CLI"),
        "--project must scope the feed"
    );
    assert!(
        events(&db, &[]).iter().any(|e| e["project"] == "LM"),
        "unscoped spans every project"
    );

    // --limit caps the feed.
    assert_eq!(events(&db, &["--limit", "2"]).len(), 2);
}
