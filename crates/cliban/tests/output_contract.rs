//! The output contract, end-to-end: the default format follows the reader.
//!
//! Piped or redirected stdout (which is what every spawn in this file is)
//! defaults to the exact JSON/NDJSON shapes `--json` produces; a TTY defaults
//! to tables and one-line confirmations. Explicit `--json` / `--table` always
//! win, `$CLIBAN_OUTPUT=json|table` pins the default in between (for
//! PTY-driven harnesses — and, mirrored here, to stand in for a TTY), and no
//! mutation succeeds silently in either mode.
//!
//! There is no PTY in this harness, so "the TTY default is Table" itself is
//! exercised via `CLIBAN_OUTPUT=table`, which shares everything but the
//! `is_terminal()` branch with the real TTY path.

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
        .join(format!("cliban_outcontract_{tag}_{nanos}.db"))
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
const JSON: &[(&str, &str)] = &[("CLIBAN_OUTPUT", "json")];

/// A board with a project, two issues, a milestone, and a label.
fn seeded(tag: &str) -> String {
    let db = tmp_db(tag);
    ok(&db, &["project", "add", "CLI", "Cliban"]);
    ok(&db, &["issue", "add", "alpha", "--project", "CLI"]);
    ok(&db, &["issue", "add", "beta", "--project", "CLI"]);
    ok(&db, &["milestone", "add", "v1", "--project", "CLI"]);
    ok(&db, &["label", "add", "prio", "--project", "CLI"]);
    db
}

#[test]
fn issue_ls_defaults_to_milestone_summary() {
    // CLI-101
    let db = seeded("issue-summary");
    ok(&db, &["issue", "edit", "CLI-1", "--milestone", "v1"]);

    let rows: Vec<serde_json::Value> = ok(&db, &["issue", "ls", "--project", "CLI", "--json"])
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        vec![
            serde_json::json!({"backlog": 1, "milestone": "v1"}),
            serde_json::json!({"backlog": 1, "milestone": null}),
        ]
    );

    let table = ok(&db, &["issue", "ls", "--project", "CLI", "--table"]);
    assert!(table.starts_with("MILESTONE  BACKLOG"), "{table}");
    assert!(table.contains("v1         1"), "{table}");
    assert!(table.contains("-          1"), "{table}");

    ok(&db, &["issue", "mv", "CLI-1", "done"]);
    ok(
        &db,
        &[
            "issue",
            "add",
            "gamma",
            "--project",
            "CLI",
            "--milestone",
            "v1",
        ],
    );
    let scoped: Vec<serde_json::Value> = ok(
        &db,
        &[
            "issue",
            "ls",
            "--project",
            "CLI",
            "--milestone",
            "v1",
            "--json",
        ],
    )
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0]["key"], "CLI-3");

    ok(&db, &["issue", "archive", "CLI-2"]);
    let all = ok(&db, &["issue", "ls", "--project", "CLI", "--all", "--json"]);
    assert_eq!(all.lines().count(), 3);
    assert!(
        all.lines().any(|line| line.contains(r#""status":"done""#)),
        "{all}"
    );
    assert!(
        all.lines().any(|line| line.contains(r#""archived":true"#)),
        "{all}"
    );
}

// --- the resolver's precedence ----------------------------------------------

#[test]
fn piped_output_defaults_to_the_exact_json_shapes() {
    let db = seeded("piped");
    // NDJSON list: byte-identical to --json (no writes in between, so even
    // the timestamps agree).
    let piped = ok(&db, &["issue", "ls"]);
    let explicit = ok(&db, &["issue", "ls", "--json"]);
    assert_eq!(piped, explicit, "piped `issue ls` must BE the --json form");
    assert!(!piped.is_empty());
    for line in piped.lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("valid NDJSON");
    }

    // CLI-99 — an inferred machine reader gets the lean shape; an explicit
    // --json request retains the complete single-entity schema.
    let piped = ok(&db, &["issue", "show", "CLI-1"]);
    let piped: serde_json::Value = serde_json::from_str(&piped).unwrap();
    let explicit: serde_json::Value =
        serde_json::from_str(&ok(&db, &["issue", "show", "CLI-1", "--json"])).unwrap();
    assert!(piped.get("description").is_none(), "{piped}");
    assert!(explicit.get("description").is_some(), "{explicit}");

    // The other groups follow the same default.
    assert_eq!(
        ok(&db, &["project", "ls"]),
        ok(&db, &["project", "ls", "--json"])
    );
    assert_eq!(
        ok(&db, &["milestone", "ls", "--project", "CLI"]),
        ok(&db, &["milestone", "ls", "--project", "CLI", "--json"])
    );
    assert_eq!(
        ok(&db, &["label", "ls", "--project", "CLI"]),
        ok(&db, &["label", "ls", "--project", "CLI", "--json"])
    );
    assert_eq!(
        ok(&db, &["activity", "--since", "1d"]),
        ok(&db, &["activity", "--since", "1d", "--json"])
    );
}

#[test]
fn piped_single_entity_reads_are_lean_but_explicit_json_is_full() {
    // CLI-99
    let db = seeded("lean-show");
    for (plain, explicit) in [
        (
            vec!["project", "show", "CLI"],
            vec!["project", "show", "CLI", "--json"],
        ),
        (
            vec!["milestone", "show", "v1", "--project", "CLI"],
            vec!["milestone", "show", "v1", "--project", "CLI", "--json"],
        ),
    ] {
        let lean: serde_json::Value = serde_json::from_str(&ok(&db, &plain)).unwrap();
        let full: serde_json::Value = serde_json::from_str(&ok(&db, &explicit)).unwrap();
        assert!(lean.get("description").is_none(), "{lean}");
        assert!(full.get("description").is_some(), "{full}");
    }

    let pinned: serde_json::Value = serde_json::from_str(&ok_env(
        &db,
        &["issue", "show", "CLI-1"],
        JSON,
    ))
    .unwrap();
    assert!(pinned.get("description").is_some(), "{pinned}");
}

#[test]
fn log_echo_omits_the_entry_and_not_found_names_the_identity() {
    // CLI-99
    let db = seeded("diet-errors");
    let echo: serde_json::Value = serde_json::from_str(&ok(
        &db,
        &["issue", "log", "CLI-1", "a deliberately long note"],
    ))
    .unwrap();
    assert_eq!(echo["key"], "CLI-1");
    assert!(echo.get("timestamp").is_some(), "{echo}");
    assert!(echo.get("entry").is_none(), "{echo}");

    for (args, identity) in [
        (vec!["issue", "show", "CLI-404"], "CLI-404"),
        (vec!["issue", "log", "CLI-404", "note"], "CLI-404"),
        (vec!["issue", "mv", "CLI-404", "done"], "CLI-404"),
        (vec!["issue", "edit", "CLI-404", "--title", "nope"], "CLI-404"),
        (vec!["issue", "cat", "CLI-404"], "CLI-404"),
        (vec!["issue", "archive", "CLI-404"], "CLI-404"),
        (
            vec!["issue", "claim", "CLI-404", "--by", "tester"],
            "CLI-404",
        ),
        (vec!["issue", "release", "CLI-404"], "CLI-404"),
        (vec!["issue", "lint", "CLI-404"], "CLI-404"),
        (
            vec!["issue", "add", "work", "--project", "MISS"],
            "MISS",
        ),
        (
            vec!["issue", "cp", "CLI-1", "--project", "MISS"],
            "MISS",
        ),
        (vec!["project", "show", "MISS"], "MISS"),
        (
            vec!["project", "edit", "MISS", "--name", "Missing"],
            "MISS",
        ),
        (
            vec!["milestone", "show", "missing", "--project", "CLI"],
            "missing",
        ),
        (
            vec![
                "milestone", "edit", "missing", "--project", "CLI", "--status", "completed",
            ],
            "missing",
        ),
    ] {
        let r = run(&db, &args);
        assert_eq!(r.code, 1, "{}", r.stderr);
        assert!(r.stderr.contains(identity), "{args:?}: {}", r.stderr);
    }
}

#[test]
fn table_flag_beats_the_env_pin_and_the_pipe() {
    let db = seeded("tableflag");
    let table = ok_env(&db, &["issue", "ls", "--table"], JSON);
    assert!(
        table.starts_with("MILESTONE"),
        "--table must win over CLIBAN_OUTPUT=json: {table}"
    );
    let json = ok_env(&db, &["issue", "ls", "--json"], TABLE);
    serde_json::from_str::<serde_json::Value>(json.lines().next().unwrap())
        .expect("--json must win over CLIBAN_OUTPUT=table");
}

#[test]
fn the_env_var_pins_the_default() {
    let db = seeded("envpin");
    let table = ok_env(&db, &["issue", "ls"], TABLE);
    assert!(
        table.starts_with("MILESTONE"),
        "CLIBAN_OUTPUT=table must override pipe detection: {table}"
    );
    let json = ok_env(&db, &["issue", "ls"], JSON);
    serde_json::from_str::<serde_json::Value>(json.lines().next().unwrap())
        .expect("CLIBAN_OUTPUT=json stays JSON");
    // An unrecognized value falls back to detection (piped → JSON) rather
    // than failing the command.
    let fallback = ok_env(&db, &["issue", "ls"], &[("CLIBAN_OUTPUT", "yaml")]);
    assert_eq!(fallback, json, "a typo'd pin must not change the contract");
}

#[test]
fn json_and_table_flags_conflict() {
    let db = seeded("conflict");
    let r = run(&db, &["issue", "ls", "--json", "--table"]);
    assert_eq!(
        r.code, 2,
        "clap must reject the contradiction: {}",
        r.stderr
    );
    assert!(r.stdout.is_empty());
}

// --- raw exemptions ----------------------------------------------------------

#[test]
fn show_section_stays_raw_markdown_in_every_mode() {
    let db = seeded("rawsection");
    ok(
        &db,
        &[
            "issue",
            "edit",
            "CLI-1",
            "--description",
            "## Spec\n\nthe spec body\n",
        ],
    );
    let piped = ok(&db, &["issue", "cat", "CLI-1", "--section", "spec"]);
    assert_eq!(piped, "\nthe spec body\n");
    // Even a JSON pin does not turn the section read into JSON: the section
    // content IS the machine format.
    assert_eq!(
        ok_env(&db, &["issue", "cat", "CLI-1", "--section", "spec"], JSON),
        piped
    );
    // Same exemption on the project side.
    ok(
        &db,
        &[
            "project",
            "edit",
            "CLI",
            "--description",
            "## Notes\n\n### A note\n\nremember this\n",
        ],
    );
    let notes = ok_env(&db, &["project", "cat", "CLI", "--section", "notes"], JSON);
    assert!(notes.contains("remember this"), "{notes}");
    assert!(!notes.trim_start().starts_with('{'), "{notes}");
}

// --- mutations: never silent, JSON echo when piped ---------------------------

#[test]
fn mv_echoes_json_when_piped_and_confirms_in_table_mode() {
    let db = seeded("mv");
    let echo = ok(&db, &["issue", "mv", "CLI-1", "in-progress"]);
    assert_eq!(
        echo.lines().count(),
        1,
        "echoes are one compact line: {echo}"
    );
    let v: serde_json::Value = serde_json::from_str(&echo).expect("mv echoes the issue as JSON");
    assert_eq!(v["key"], "CLI-1");
    assert_eq!(v["status"], "in-progress");
    // The echo is the lean row shape — no body — but its updated_at keeps
    // stored precision, so it IS a valid --if-updated-at CAS token.
    assert!(v.get("description").is_none(), "{v}");
    let shown: serde_json::Value =
        serde_json::from_str(&ok(&db, &["issue", "show", "CLI-1", "--json"])).unwrap();
    assert_eq!(
        v["updated_at"], shown["updated_at"],
        "echo must be a CAS token"
    );

    let confirm = ok_env(&db, &["issue", "mv", "CLI-1", "in-review"], TABLE);
    assert_eq!(confirm, "moved CLI-1: in-progress → in-review\n");
}

#[test]
fn archive_and_unarchive_confirm_or_echo() {
    let db = seeded("arch");
    let echo = ok(&db, &["issue", "archive", "CLI-2"]);
    let v: serde_json::Value = serde_json::from_str(&echo).unwrap();
    assert_eq!(v["key"], "CLI-2");
    assert_eq!(v["archived"], true);
    let confirm = ok_env(&db, &["issue", "unarchive", "CLI-2"], TABLE);
    assert_eq!(confirm, "unarchived CLI-2\n");
}

#[test]
fn label_add_and_rm_confirm_or_echo() {
    let db = seeded("label");
    let echo = ok(&db, &["label", "add", "bug", "--project", "CLI"]);
    let v: serde_json::Value = serde_json::from_str(&echo).unwrap();
    assert_eq!(v["name"], "bug");
    assert_eq!(v["project"], "CLI");
    assert_eq!(
        ok_env(&db, &["label", "rm", "bug", "--project", "CLI"], TABLE),
        "removed label bug from CLI\n"
    );
    let echo = ok(&db, &["label", "rm", "prio", "--project", "CLI"]);
    let v: serde_json::Value = serde_json::from_str(&echo).unwrap();
    assert_eq!(v["removed"], true);
}

#[test]
fn project_edit_and_archive_confirm_or_echo() {
    let db = seeded("projedit");
    let echo = ok(&db, &["project", "edit", "CLI", "--name", "Renamed"]);
    let v: serde_json::Value = serde_json::from_str(&echo).expect("project edit echoes JSON");
    assert_eq!(v["key"], "CLI");
    assert_eq!(v["name"], "Renamed");
    assert_eq!(
        ok_env(&db, &["project", "edit", "CLI", "--name", "Again"], TABLE),
        "updated project CLI\n"
    );
    let echo = ok(&db, &["project", "archive", "CLI"]);
    let v: serde_json::Value = serde_json::from_str(&echo).unwrap();
    assert_eq!(v["archived"], true);
    assert_eq!(
        ok_env(&db, &["project", "unarchive", "CLI"], TABLE),
        "unarchived project CLI\n"
    );
}

#[test]
fn milestone_edit_confirms_or_echoes() {
    let db = seeded("msedit");
    let echo = ok(
        &db,
        &[
            "milestone",
            "edit",
            "v1",
            "--project",
            "CLI",
            "--status",
            "completed",
        ],
    );
    let v: serde_json::Value = serde_json::from_str(&echo).expect("milestone edit echoes JSON");
    assert_eq!(v["name"], "v1");
    assert_eq!(v["status"], "completed");
    assert_eq!(v["project"], "CLI");
    assert_eq!(
        ok_env(
            &db,
            &[
                "milestone",
                "edit",
                "v1",
                "--project",
                "CLI",
                "--status",
                "open",
            ],
            TABLE
        ),
        "updated milestone v1 in CLI\n"
    );
}

#[test]
fn project_note_add_confirms_or_echoes() {
    let db = seeded("noteadd");
    let echo = ok(
        &db,
        &[
            "project", "note", "add", "CLI", "Lesson", "--body", "learned",
        ],
    );
    let v: serde_json::Value = serde_json::from_str(&echo).expect("note add echoes JSON");
    assert_eq!(v["note"], "Lesson");
    let confirm = ok_env(
        &db,
        &["project", "note", "add", "CLI", "Another", "--body", "more"],
        TABLE,
    );
    assert_eq!(confirm, "added note \"Another\" to CLI ## Notes\n");
}

#[test]
fn no_listed_mutation_succeeds_silently_in_either_mode() {
    // The whole mutation set the spec names, both modes: stdout must never be
    // empty on success.
    let mutations: &[&[&str]] = &[
        &["issue", "mv", "CLI-1", "in-progress"],
        &["issue", "archive", "CLI-1"],
        &["issue", "unarchive", "CLI-1"],
        &["label", "add", "fresh", "--project", "CLI"],
        &["label", "rm", "fresh", "--project", "CLI"],
        &["project", "edit", "CLI", "--name", "Other"],
        &[
            "milestone",
            "edit",
            "v1",
            "--project",
            "CLI",
            "--target",
            "2027-01-01",
        ],
        &["project", "note", "add", "CLI", "N", "--body", "b"],
    ];
    for env in [JSON, TABLE] {
        let db = seeded(if env == JSON { "silentj" } else { "silentt" });
        for m in mutations {
            let out = ok_env(&db, m, env);
            assert!(
                !out.trim().is_empty(),
                "`cliban {}` (env {:?}) succeeded silently",
                m.join(" "),
                env
            );
        }
    }
}

// --- confirmations name the entity and the change ----------------------------

#[test]
fn confirmations_name_the_entity_and_the_change() {
    let db = seeded("naming");
    for (args, needles) in [
        (
            vec!["issue", "mv", "CLI-1", "in-progress"],
            vec!["CLI-1", "backlog", "in-progress"],
        ),
        (vec!["issue", "archive", "CLI-2"], vec!["archived", "CLI-2"]),
        (
            vec!["label", "add", "hot", "--project", "CLI"],
            vec!["hot", "CLI"],
        ),
    ] {
        let out = ok_env(&db, &args, TABLE);
        for n in needles {
            assert!(
                out.contains(n),
                "`cliban {}` confirmation must name {n:?}: {out}",
                args.join(" ")
            );
        }
    }
}

// --- the list-row diet ------------------------------------------------------

/// List rows are lean (absent means default), single-entity output is
/// complete, and `--full` restores the complete shape on a list.
#[test]
fn list_rows_are_lean_and_full_detail_is_complete() {
    let db = seeded("lean");
    let row: serde_json::Value = serde_json::from_str(
        ok(&db, &["issue", "ls", "-p", "CLI", "--all"])
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    for absent in [
        "description",
        "git_branch_name",
        "position",
        "created_at",
        "milestone",
        "parent",
        "due_date",
        "labels",
        "relations",
        "archived",
    ] {
        assert!(row.get(absent).is_none(), "lean row leaked {absent}: {row}");
    }
    let full: serde_json::Value = serde_json::from_str(
        ok(&db, &["issue", "ls", "-p", "CLI", "--full"])
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    for present in ["description", "git_branch_name", "position", "created_at"] {
        assert!(full.get(present).is_some(), "--full missing {present}");
    }
    // Explicit --json stays the complete shape with null-not-absent optionals.
    let shown: serde_json::Value =
        serde_json::from_str(&ok(&db, &["issue", "show", "CLI-1", "--json"])).unwrap();
    assert!(shown.get("milestone").is_some_and(|v| v.is_null()));
}

/// Unscoped `milestone ls` is a per-project count summary; detail flags
/// require `--project`.
#[test]
fn unscoped_milestone_ls_is_a_summary() {
    let db = seeded("mssum");
    let out = ok(&db, &["milestone", "ls"]);
    assert_eq!(
        out.trim(),
        r#"{"milestones":1,"open":1,"project":"CLI"}"#,
        "got {out}"
    );
    for flags in [["--stats"], ["--full"]] {
        let r = run(&db, &[&["milestone", "ls"], &flags[..]].concat());
        assert_eq!(r.code, 2, "unscoped {} should be exit 2", flags[0]);
        assert!(r.stderr.contains("--project"), "got {}", r.stderr);
    }
    // Scoped, the row form works and stays lean.
    let row: serde_json::Value = serde_json::from_str(
        ok(&db, &["milestone", "ls", "-p", "CLI"])
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    assert!(row.get("created_at").is_none() && row.get("description").is_none());
    // --stats rows follow the same diet.
    let stat: serde_json::Value = serde_json::from_str(
        ok(&db, &["milestone", "ls", "-p", "CLI", "--stats"])
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    assert!(stat.get("done_count").is_some() && stat.get("description").is_none());
}

/// `-p` / `-s` / `-m` are exact synonyms of the long filter flags.
#[test]
fn short_filter_flags_match_their_long_forms() {
    let db = seeded("shorts");
    assert_eq!(
        ok(&db, &["issue", "ls", "-p", "CLI", "-s", "backlog"]),
        ok(
            &db,
            &["issue", "ls", "--project", "CLI", "--status", "backlog"]
        )
    );
    assert_eq!(
        ok(&db, &["issue", "ls", "-p", "CLI", "-m", "v1"]),
        ok(
            &db,
            &["issue", "ls", "--project", "CLI", "--milestone", "v1"]
        )
    );
    assert_eq!(
        ok(&db, &["activity", "-p", "CLI"]),
        ok(&db, &["activity", "--project", "CLI"])
    );
}
