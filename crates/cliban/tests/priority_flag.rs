//! `--priority` is one value set across every flag that takes it.
//!
//! It used to be a bare string, and the paths disagreed: `edit`, `ls`, and
//! `import` trimmed and lowercased it, while `add` handed the raw string to
//! core's exact, case-sensitive check. `--priority HIGH` was accepted by
//! `edit` and rejected by `add`, and `add`'s rejection ("validation error: is
//! invalid") did not even name the valid values.

use std::process::Command;

fn run(db: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cliban"))
        .arg("--db")
        .arg(db)
        .env_remove("CLIBAN_OUTPUT")
        .env_remove("CLIBAN_PROJECT")
        .args(args)
        .output()
        .expect("run cliban")
}

fn stdout(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn board(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("cliban_priority_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db = root.join("board.db");
    assert!(run(&db, &["project", "add", "PR", "Pri"]).status.success());
    db
}

const VALUES: &str = "[possible values: none, low, medium, high, urgent]";

#[test]
fn help_lists_the_accepted_priorities() {
    let db = board("help");
    for cmd in [
        vec!["issue", "add", "--help"],
        vec!["issue", "edit", "--help"],
        vec!["issue", "ls", "--help"],
    ] {
        let o = run(&db, &cmd);
        assert!(o.status.success());
        assert!(
            stdout(&o).contains(VALUES),
            "{cmd:?} help must list the values, got:\n{}",
            stdout(&o)
        );
    }
}

#[test]
fn an_invalid_priority_is_rejected_by_name_on_every_path() {
    let db = board("invalid");
    assert!(run(&db, &["issue", "add", "seed", "-p", "PR"])
        .status
        .success());
    for cmd in [
        vec!["issue", "add", "x", "-p", "PR", "--priority", "bogus"],
        vec!["issue", "edit", "PR-1", "--priority", "bogus"],
        vec!["issue", "ls", "-p", "PR", "--priority", "bogus"],
    ] {
        let o = run(&db, &cmd);
        assert!(!o.status.success(), "{cmd:?} must fail");
        assert!(
            stderr(&o).contains(VALUES),
            "{cmd:?} must name the valid values, got:\n{}",
            stderr(&o)
        );
    }
}

#[test]
fn every_path_accepts_the_same_spellings() {
    // Case-insensitivity is what `edit`, `ls`, and `import` already accepted;
    // `add` is the one that used to refuse it.
    let db = board("spellings");
    for (i, spelling) in ["high", "HIGH", "HiGh"].iter().enumerate() {
        let o = run(
            &db,
            &[
                "issue",
                "add",
                &format!("t{i}"),
                "-p",
                "PR",
                "--priority",
                spelling,
                "--json",
            ],
        );
        assert!(o.status.success(), "add {spelling}: {}", stderr(&o));
        let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
        assert_eq!(v["priority"], "high", "stored spelling is normalized");
    }

    let o = run(
        &db,
        &["issue", "edit", "PR-1", "--priority", "URGENT", "--json"],
    );
    assert!(o.status.success(), "edit: {}", stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["priority"], "urgent");

    let o = run(
        &db,
        &["issue", "ls", "-p", "PR", "--priority", "HIGH", "--json"],
    );
    assert!(o.status.success(), "ls: {}", stderr(&o));
    assert_eq!(
        stdout(&o).lines().count(),
        2,
        "the two remaining high issues match the filter"
    );
}
