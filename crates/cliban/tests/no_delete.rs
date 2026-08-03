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
fn rm_is_not_a_command_for_work_items() {
    // The Go-era muscle-memory aliases are gone: deleting a work item is not
    // a thing cliban does under any spelling. clap refuses the subcommand.
    let db = seeded();
    for args in [
        vec!["issue", "rm", "CLI-1"],
        vec!["project", "rm", "CLI"],
        vec!["milestone", "rm", "--project", "CLI", "--name", "v1"],
    ] {
        let (_, err, code) = run(&db, &args);
        assert_ne!(code, 0, "`{args:?}` must be rejected");
        assert!(
            err.contains("unrecognized subcommand") || err.contains("error:"),
            "`{args:?}` should fail as an unknown subcommand: {err}"
        );
    }
    // A label is a tag, not a work item: its rm is real and stays.
    assert_eq!(run(&db, &["label", "add", "bug", "--project", "CLI"]).2, 0);
    assert_eq!(run(&db, &["label", "rm", "bug", "--project", "CLI"]).2, 0);
}

#[test]
fn archiving_is_recorded_and_reversible() {
    let db = seeded();
    assert_eq!(run(&db, &["issue", "archive", "CLI-1"]).2, 0);

    // Still there, just archived — and the archive landed on the timeline.
    let issue = run(&db, &["issue", "show", "CLI-1", "--json"]).0;
    assert!(issue.contains("keep me"), "{issue}");
    assert!(issue.contains("\"archived\": true"), "{issue}");
    let feed = run(&db, &["activity", "--archived", "--json"]).0;
    assert!(
        feed.lines()
            .any(|l| l.contains("\"kind\":\"archive\"") && l.contains("archived")),
        "the archive must be recorded: {feed}"
    );

    // …and reversible, so nothing is ever lost.
    assert_eq!(run(&db, &["issue", "unarchive", "CLI-1"]).2, 0);
    assert!(run(&db, &["issue", "ls", "--json"]).0.contains("CLI-1"));
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
