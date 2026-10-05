//! `--brief` end-to-end: short, speakable summaries that stay within the line
//! budget even when an issue's description is enormous.

use std::process::Command;

fn tmp(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("cliban_brief_{tag}_{nanos}"))
}

fn run(db: &str, args: &[&str]) -> (String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_cliban"))
        .arg("--db")
        .arg(db)
        .env_remove("CLIBAN_DB")
        .env_remove("XDG_DATA_HOME")
        .env_remove("CLIBAN_ACTOR")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("CLIBAN_OUTPUT")
        .env_remove("CLIBAN_PROJECT")
        .args(args)
        .output()
        .expect("run cliban");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

fn ok(db: &str, args: &[&str]) -> String {
    let (out, code) = run(db, args);
    assert_eq!(code, 0, "`cliban {}` failed", args.join(" "));
    out
}

const DEEP: &str = "ZZDEEPMARKERZZ";

fn fat_description() -> String {
    let filler = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ";
    let mut d = String::from(
        "## Spec\n\nEnsure the **gizmo** frobnicates quickly. A second sentence follows. \
         The third sentence must not appear.\n\n## Plan\n\n",
    );
    d.push_str(&format!("{DEEP} {}\n\n", filler.repeat(250)));
    d.push_str("## Notes\n\n");
    d.push_str(&filler.repeat(250));
    d.push_str("\n\n## Activity Log\n\n");
    for n in 0..100 {
        d.push_str(&format!(
            "- 2026-01-01T00:{:02}Z — {DEEP} old entry {n}\n",
            n % 60
        ));
    }
    assert!(d.len() >= 28_000, "fixture too small: {}", d.len());
    d
}

/// Returns the db path.
fn seeded() -> String {
    let db = tmp("db").to_string_lossy().to_string();
    let file = tmp("fat");
    std::fs::write(&file, fat_description()).unwrap();
    ok(&db, &["project", "add", "BR", "Brief Test"]);
    ok(
        &db,
        &[
            "milestone",
            "add",
            "Voice",
            "-p",
            "BR",
            "--target",
            "2026-11-01",
        ],
    );
    ok(&db, &["issue", "add", "Blocker thing", "-p", "BR"]);
    ok(
        &db,
        &[
            "issue",
            "add",
            "Fat issue",
            "-p",
            "BR",
            "-m",
            "Voice",
            "-s",
            "in-progress",
            "--priority",
            "high",
            "--blocked-by",
            "BR-1",
            "--description-file",
            file.to_str().unwrap(),
        ],
    );
    ok(
        &db,
        &[
            "issue",
            "add",
            "Review thing",
            "-p",
            "BR",
            "-m",
            "Voice",
            "-s",
            "in-review",
        ],
    );
    ok(
        &db,
        &[
            "issue",
            "add",
            "Stuck thing",
            "-p",
            "BR",
            "-m",
            "Voice",
            "-s",
            "blocked",
        ],
    );
    ok(
        &db,
        &[
            "issue",
            "add",
            "Done thing",
            "-p",
            "BR",
            "-m",
            "Voice",
            "-s",
            "done",
        ],
    );
    ok(&db, &["issue", "log", "BR-2", "wired up the gizmo"]);
    let _ = std::fs::remove_file(file);
    db
}

fn check(out: &str, wants: &[&str]) {
    assert!(out.lines().count() <= 40, "too long:\n{out}");
    assert!(!out.contains(DEEP), "leaked description:\n{out}");
    assert!(
        out.contains("ago") || out.contains("just now"),
        "no age phrasing:\n{out}"
    );
    for w in wants {
        assert!(out.contains(w), "missing {w:?} in:\n{out}");
    }
}

#[test]
fn issue_show_brief_of_fat_issue() {
    let db = seeded();
    let out = ok(&db, &["issue", "show", "BR-2", "--brief"]);
    check(
        &out,
        &[
            "BR-2: Fat issue.",
            "Status in-progress, priority high, milestone Voice.",
            "Spec: Ensure the gizmo frobnicates quickly. A second sentence follows.",
            "Blocked by BR-1 (backlog).",
            "Latest log just now: wired up the gizmo.",
        ],
    );
    assert!(!out.contains("third sentence"));
    // The bare spare shares the flag.
    assert_eq!(ok(&db, &["show", "BR-2", "--brief"]), out);
}

#[test]
fn board_briefs_count_and_list() {
    let db = seeded();
    for args in [
        vec!["project", "show", "BR", "--brief"],
        vec!["milestone", "show", "Voice", "-p", "BR", "--brief"],
        vec!["issue", "ls", "-p", "BR", "--brief"],
        vec!["ls", "-p", "BR", "--all", "--brief"],
    ] {
        let out = ok(&db, &args);
        check(
            &out,
            &[
                "In progress: BR-2 Fat issue, updated",
                "Blocked: BR-4 Stuck thing",
                "In review: BR-3 Review thing",
                "Newest change",
            ],
        );
        assert!(!out.contains("BR-1 Blocker"), "backlog listed:\n{out}");
    }
    let out = ok(&db, &["project", "show", "BR", "--brief"]);
    assert!(out.contains("Project BR, Brief Test."), "{out}");
    assert!(
        out.contains("5 issues: 1 in progress, 1 blocked, 1 in review, 1 in backlog, 1 done."),
        "{out}"
    );
    let out = ok(&db, &["milestone", "show", "Voice", "-p", "BR", "--brief"]);
    assert!(
        out.contains("Milestone Voice in BR, status open, target 2026-11-01."),
        "{out}"
    );
}

#[test]
fn activity_brief_is_short_and_newest_first() {
    let db = seeded();
    let out = ok(&db, &["activity", "-p", "BR", "--brief"]);
    check(&out, &["changes since 1d.", "BR-2:", "wired up the gizmo"]);
}

#[test]
fn brief_conflicts_with_json_and_table() {
    let db = seeded();
    for args in [
        vec!["issue", "show", "BR-2", "--brief", "--json"],
        vec!["issue", "ls", "--brief", "--table"],
        vec!["project", "show", "BR", "--brief", "--json"],
        vec![
            "milestone",
            "show",
            "Voice",
            "-p",
            "BR",
            "--brief",
            "--json",
        ],
        vec!["activity", "--brief", "--json"],
    ] {
        assert_eq!(run(&db, &args).1, 2, "{args:?}");
    }
}
