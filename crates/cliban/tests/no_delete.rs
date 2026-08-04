//! cliban archives; it does not delete.
//!
//! A deleted row takes its timeline with it, and a history with holes is worse
//! than no history. `rm` still works — reaching for it archives (or, for a
//! milestone, cancels) and says so, rather than spending the caller a turn on
//! a refusal.

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
        .join(format!("cliban_nodelete_{tag}_{nanos}.db"))
        .to_string_lossy()
        .to_string()
}

fn run(db: &str, args: &[&str]) -> (String, String, i32) {
    let out = Command::new(bin())
        .arg("--db")
        .arg(db)
        .env_remove("CLIBAN_DB")
        .env_remove("XDG_DATA_HOME")
        .env_remove("CLIBAN_OUTPUT")
        .args(args)
        .output()
        .expect("run cliban");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

fn seeded() -> String {
    let db = tmp_db("board");
    for args in [
        vec!["project", "add", "CLI", "--name", "Cliban"],
        vec!["milestone", "add", "--project", "CLI", "--name", "v1"],
        vec!["issue", "add", "--project", "CLI", "--title", "keep me"],
    ] {
        assert_eq!(run(&db, &args).2, 0, "seed failed: {args:?}");
    }
    db
}

#[test]
fn rm_archives_and_says_so_rather_than_refusing() {
    let db = seeded();
    for (args, expect) in [
        (vec!["issue", "rm", "CLI-1"], "archived CLI-1"),
        (vec!["project", "rm", "CLI"], "archived project CLI"),
        (
            vec!["milestone", "rm", "--project", "CLI", "--name", "v1"],
            "cancelled milestone v1",
        ),
    ] {
        let (out, _, code) = run(&db, &args);
        assert_eq!(code, 0, "`{args:?}` must succeed");
        assert!(
            out.contains(expect),
            "`{args:?}` must report {expect:?}: {out}"
        );
        assert!(
            out.contains("instead of deleting"),
            "`{args:?}` must be explicit that nothing was deleted: {out}"
        );
        assert!(
            out.contains("undo: cliban"),
            "`{args:?}` must name the undo: {out}"
        );
    }
}

#[test]
fn rm_archives_rather_than_destroying() {
    let db = seeded();
    run(&db, &["issue", "rm", "CLI-1"]);
    run(&db, &["project", "rm", "CLI"]);
    run(
        &db,
        &["milestone", "rm", "--project", "CLI", "--name", "v1"],
    );

    // Every row is still there, just archived/cancelled.
    let issue = run(&db, &["issue", "show", "CLI-1", "--json"]).0;
    assert!(issue.contains("keep me"), "{issue}");
    assert!(issue.contains("\"archived\": true"), "{issue}");
    assert!(
        !run(&db, &["project", "ls", "--json"]).0.contains("CLI"),
        "an archived project leaves the default listing"
    );
    assert!(
        run(&db, &["project", "ls", "--archived", "--json"])
            .0
            .contains("CLI"),
        "…but the project itself is still there"
    );
    let ms = run(&db, &["milestone", "ls", "--project", "CLI", "--table"]).0;
    assert!(ms.contains("v1") && ms.contains("cancelled"), "{ms}");

    // …and each is reversible, so `rm` costs nothing irreversible.
    assert_eq!(run(&db, &["issue", "unarchive", "CLI-1"]).2, 0);
    assert!(run(&db, &["issue", "ls", "--json"]).0.contains("CLI-1"));
}

#[test]
fn an_rm_lands_on_the_timeline_like_any_other_archive() {
    let db = seeded();
    run(&db, &["issue", "rm", "CLI-1"]);
    let feed = run(&db, &["activity", "--archived", "--json"]).0;
    assert!(
        feed.lines()
            .any(|l| l.contains("\"kind\":\"archive\"") && l.contains("archived")),
        "the archive must be recorded: {feed}"
    );
}

#[test]
fn project_rm_still_accepts_the_old_force_flag() {
    // A unix reflex often arrives with --force; archiving needs no force, but
    // rejecting the flag would cost exactly the turn this alias exists to
    // save.
    let db = seeded();
    let (out, err, code) = run(&db, &["project", "rm", "CLI", "--force"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("archived project CLI"), "{out}");
}

#[test]
fn label_rm_is_real_and_stays() {
    // A label is a tag, not a work item: its rm genuinely deletes.
    let db = seeded();
    assert_eq!(run(&db, &["label", "add", "bug", "--project", "CLI"]).2, 0);
    assert_eq!(run(&db, &["label", "rm", "bug", "--project", "CLI"]).2, 0);
}

#[test]
fn help_does_not_advertise_rm() {
    let db = seeded();
    for group in ["issue", "project", "milestone"] {
        let (help, _, _) = run(&db, &[group, "--help"]);
        let commands: Vec<&str> = help
            .lines()
            .skip_while(|l| !l.starts_with("Commands:"))
            .skip(1)
            .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
            .filter_map(|l| l.split_whitespace().next())
            .collect();
        assert!(
            !commands.contains(&"rm"),
            "`{group} --help` still lists rm: {commands:?}"
        );
        assert!(
            commands.contains(&"add"),
            "sanity: parsed the command list for {group}: {commands:?}"
        );
    }
}

#[test]
fn archiving_remains_the_supported_path_and_is_reversible() {
    let db = seeded();
    assert_eq!(run(&db, &["issue", "archive", "CLI-1"]).2, 0);
    assert!(
        !run(&db, &["issue", "ls", "--json"]).0.contains("CLI-1"),
        "archived issues leave the default list"
    );
    assert!(
        run(&db, &["issue", "ls", "--archived", "--json"])
            .0
            .contains("CLI-1"),
        "…but are still there"
    );
    assert_eq!(run(&db, &["issue", "unarchive", "CLI-1"]).2, 0);
    assert!(run(&db, &["issue", "ls", "--json"]).0.contains("CLI-1"));
}

/// Labels are not work items — they carry no timeline, and detaching one
/// destroys no history — so `label rm` is deliberately still supported.
#[test]
fn label_rm_survives_because_a_label_has_no_history() {
    let db = seeded();
    assert_eq!(run(&db, &["label", "add", "bug", "--project", "CLI"]).2, 0);
    assert_eq!(run(&db, &["issue", "edit", "CLI-1", "--label", "bug"]).2, 0);
    assert_eq!(run(&db, &["label", "rm", "bug", "--project", "CLI"]).2, 0);
    // The label is gone; the issue it was attached to is not.
    assert!(!run(&db, &["label", "ls", "--project", "CLI", "--json"])
        .0
        .contains("bug"));
    assert!(run(&db, &["issue", "show", "CLI-1", "--json"])
        .0
        .contains("keep me"));
}
