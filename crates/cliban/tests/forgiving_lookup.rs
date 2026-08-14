use std::process::Command;

fn bin() -> String {
    env!("CARGO_BIN_EXE_cliban").to_string()
}

fn db(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("cliban_lookup_{tag}_{nanos}.db"))
        .to_string_lossy()
        .to_string()
}

fn run(db: &str, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .arg("--db")
        .arg(db)
        .args(args)
        .env_remove("CLIBAN_DB")
        .env_remove("CLIBAN_PROJECT")
        .env_remove("CLIBAN_OUTPUT")
        .output()
        .expect("run cliban")
}

fn ok(db: &str, args: &[&str]) -> String {
    let out = run(db, args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn seeded(tag: &str) -> String {
    let db = db(tag);
    ok(&db, &["project", "add", "CLI", "Cliban"]);
    db
}

#[test]
fn milestone_show_accepts_case_and_one_fuzzy_match() {
    let db = seeded("milestone");
    ok(
        &db,
        &["milestone", "add", "Deterministic integration", "-p", "CLI"],
    );

    for query in ["DETERMINISTIC INTEGRATION", "Deterministc integration"] {
        let out = ok(&db, &["milestone", "show", query, "-p", "CLI", "--json"]);
        let row: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(row["name"], "Deterministic integration");
    }
}

#[test]
fn ambiguous_milestone_lookup_lists_candidates_without_choosing() {
    let db = seeded("ambiguous");
    for name in ["Dropdown cleanup", "Dropdown redesign"] {
        ok(&db, &["milestone", "add", name, "-p", "CLI"]);
    }

    let out = run(
        &db,
        &["milestone", "show", "Dropdown", "-p", "CLI", "--table"],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Dropdown cleanup"), "{err}");
    assert!(err.contains("Dropdown redesign"), "{err}");
}

#[test]
fn sole_weak_milestone_match_is_a_json_candidate_not_a_selection() {
    let db = seeded("weak");
    ok(&db, &["milestone", "add", "Dropdown cleanup", "-p", "CLI"]);

    let out = run(
        &db,
        &["milestone", "show", "d", "-p", "CLI", "--json"],
    );
    assert_eq!(out.status.code(), Some(1));
    let row: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(row, serde_json::json!({"name": "Dropdown cleanup", "project": "CLI"}));
}

#[test]
fn forgiving_name_is_shared_by_milestone_commands() {
    let db = seeded("shared");
    ok(&db, &["milestone", "add", "Dropdown cleanup", "-p", "CLI"]);

    ok(
        &db,
        &[
            "milestone",
            "edit",
            "Dropdwn cleanup",
            "-p",
            "CLI",
            "--status",
            "completed",
        ],
    );
    ok(
        &db,
        &[
            "milestone",
            "log",
            "DROPDOWN CLEANUP",
            "note",
            "-p",
            "CLI",
        ],
    );
    ok(
        &db,
        &["milestone", "waves", "Dropdwn cleanup", "-p", "CLI"],
    );
}

#[test]
fn issue_search_defaults_to_ten_but_explicit_limit_wins() {
    let db = seeded("search_limit");
    for n in 0..12 {
        ok(
            &db,
            &["issue", "add", &format!("Dropdown fix {n}"), "-p", "CLI"],
        );
    }

    let default = ok(&db, &["issue", "ls", "--search", "Dropdown", "-p", "CLI"]);
    assert_eq!(default.lines().count(), 10);
    let explicit = ok(
        &db,
        &[
            "issue", "ls", "--search", "Dropdown", "--limit", "12", "-p", "CLI",
        ],
    );
    assert_eq!(explicit.lines().count(), 12);
}
