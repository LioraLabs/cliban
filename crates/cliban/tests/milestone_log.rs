//! `cliban milestone log`: the milestone-scoped sibling of
//! `issue log`.
//!
//! Black-box over the real binary, because the two things this command has to
//! get right are only observable from outside it: the exact markdown line it
//! splices into someone else's description, and what happens when several
//! processes append at once.

use std::process::Command;

fn bin() -> String {
    env!("CARGO_BIN_EXE_cliban").to_string()
}

fn tmp_db(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("cliban_mlog_{tag}_{nanos}.db"));
    path.to_string_lossy().to_string()
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

/// A scrubbed environment: never the developer's real board, never an ambient
/// actor, never a `CLIBAN_OUTPUT` pin that would defeat the piped-JSON default.
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
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
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

/// A board with one project and one milestone carrying a real description.
fn seeded(tag: &str, description: &str) -> String {
    let db = tmp_db(tag);
    ok(&db, &["project", "add", "CLI", "Cliban"]);
    ok(
        &db,
        &[
            "milestone",
            "add",
            "Deterministic integration",
            "-p",
            "CLI",
            "--description",
            description,
        ],
    );
    db
}

fn description(db: &str) -> String {
    let out = ok(
        db,
        &[
            "milestone",
            "show",
            "Deterministic integration",
            "-p",
            "CLI",
            "--json",
        ],
    );
    let v: serde_json::Value = serde_json::from_str(&out).expect("milestone show --json");
    v["description"].as_str().expect("description").to_string()
}

/// The `- <ts> — <msg>` entries of the `## Activity Log` section, in order.
fn entries(db: &str) -> Vec<String> {
    let desc = description(db);
    let (_, after) = desc
        .split_once("## Activity Log")
        .unwrap_or_else(|| panic!("no ## Activity Log section in:\n{desc}"));
    after
        .lines()
        .take_while(|l| !l.starts_with("## "))
        .filter(|l| l.starts_with("- "))
        .map(str::to_string)
        .collect()
}

#[test]
fn first_entry_creates_the_section_and_leaves_the_rest_byte_identical() {
    let spec = "## Spec\n\nthe milestone's live spec\n";
    let db = seeded("create", spec);

    ok(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "wave 1 dispatched",
        ],
    );

    let desc = description(&db);
    assert!(
        desc.starts_with(spec),
        "the pre-existing description must survive verbatim:\n{desc}"
    );
    let got = entries(&db);
    assert_eq!(got.len(), 1, "{got:?}");
    // Same line shape `issue log` writes: `- <RFC-3339 to the minute> — <msg>`.
    let entry = &got[0];
    assert!(
        entry.ends_with(" — wave 1 dispatched"),
        "entry not in `- <ts> — <msg>` form: {entry:?}"
    );
    let stamp = entry
        .trim_start_matches("- ")
        .split(" — ")
        .next()
        .expect("timestamp");
    assert!(
        chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%dT%H:%MZ").is_ok(),
        "timestamp {stamp:?} is not minute-precision RFC-3339 UTC"
    );
}

#[test]
fn subsequent_entries_append_in_order_under_one_heading() {
    let db = seeded("append", "## Spec\n\nbody\n");
    for msg in ["first", "second", "third"] {
        ok(
            &db,
            &[
                "milestone",
                "log",
                "Deterministic integration",
                "-p",
                "CLI",
                msg,
            ],
        );
    }

    let got = entries(&db);
    assert_eq!(got.len(), 3, "{got:?}");
    for (entry, expected) in got.iter().zip(["first", "second", "third"]) {
        assert!(
            entry.ends_with(&format!(" — {expected}")),
            "out of order: {got:?}"
        );
    }
    assert_eq!(
        description(&db).matches("## Activity Log").count(),
        1,
        "a second heading means the section was recreated rather than appended to"
    );
}

#[test]
fn logging_onto_a_milestone_with_no_description_still_produces_a_section() {
    let db = seeded("empty", "");
    ok(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "started",
        ],
    );
    let desc = description(&db);
    assert!(
        desc.starts_with("## Activity Log\n\n- "),
        "empty description should yield a bare section:\n{desc:?}"
    );
}

#[test]
fn the_section_survives_show_and_a_later_edit_of_another_field() {
    let db = seeded("read-paths", "## Spec\n\nbody\n");
    ok(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "landing attempted",
        ],
    );

    // --json still parses, and reports the section in `description`.
    let json = ok(
        &db,
        &[
            "milestone",
            "show",
            "Deterministic integration",
            "-p",
            "CLI",
            "--json",
        ],
    );
    let v: serde_json::Value = serde_json::from_str(&json).expect("show --json parses");
    assert!(v["description"]
        .as_str()
        .unwrap()
        .contains("landing attempted"));

    // The human read path prints it too.
    let human = ok(
        &db,
        &[
            "milestone",
            "show",
            "Deterministic integration",
            "-p",
            "CLI",
            "--table",
        ],
    );
    assert!(human.contains("landing attempted"), "{human}");

    // And a non-description edit does not disturb it.
    ok(
        &db,
        &[
            "milestone",
            "edit",
            "Deterministic integration",
            "-p",
            "CLI",
            "--target",
            "2026-12-01",
        ],
    );
    assert_eq!(entries(&db).len(), 1);

    // `milestone ls --stats` and `waves` also still work on this milestone.
    ok(&db, &["milestone", "ls", "-p", "CLI", "--stats"]);
    ok(
        &db,
        &[
            "milestone",
            "waves",
            "Deterministic integration",
            "-p",
            "CLI",
        ],
    );
}

/// The ticket promises the two logs write the same line, so one parser reads
/// both. That holds structurally today because both go through
/// `descmd::append_activity_log` — but "structurally" is exactly what quietly
/// stops being true. So pin it against the real `issue log`, on one board,
/// with one message: only the timestamp *value* may differ (the two writes are
/// a moment apart), never its shape, the list marker, or the separator.
#[test]
fn the_entry_line_is_the_line_issue_log_writes() {
    let db = seeded("mirror", "## Spec\n\nbody\n");
    ok(&db, &["issue", "add", "a ticket", "-p", "CLI"]);
    let msg = "[cliban-flow] milestone start CLI: set up (2 waves)";

    ok(&db, &["issue", "log", "CLI-1", msg]);
    ok(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            msg,
        ],
    );

    let issue_section = ok(&db, &["issue", "cat", "CLI-1", "--section", "activity"]);
    let issue_line = issue_section
        .lines()
        .find(|l| l.starts_with("- "))
        .expect("issue log wrote a list item")
        .to_string();
    let milestone_line = entries(&db).pop().expect("milestone log wrote an entry");

    // `- <stamp> — <message>` split into its two variable parts; a panic here
    // means the fixed parts have already diverged.
    fn split(line: &str) -> (String, String) {
        let rest = line.strip_prefix("- ").expect("list marker");
        let (stamp, msg) = rest.split_once(" — ").expect("` — ` separator");
        (stamp.to_string(), msg.to_string())
    }
    let (issue_stamp, issue_msg) = split(&issue_line);
    let (milestone_stamp, milestone_msg) = split(&milestone_line);

    assert_eq!(issue_msg, msg);
    assert_eq!(
        issue_msg, milestone_msg,
        "one message, two renderings: {issue_line:?} vs {milestone_line:?}"
    );
    assert_eq!(
        issue_stamp.len(),
        milestone_stamp.len(),
        "timestamp shapes differ: {issue_line:?} vs {milestone_line:?}"
    );
    for stamp in [&issue_stamp, &milestone_stamp] {
        assert!(
            chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%dT%H:%MZ").is_ok(),
            "timestamp {stamp:?} is not the shape the other side writes"
        );
    }

    // The JSON echoes differ only where the subject differs: an issue has a
    // key, a milestone has a name and a project. Everything else is shared,
    // and a rename on either side breaks this.
    let echo = |raw: String| {
        let v: serde_json::Value = serde_json::from_str(raw.trim()).expect("log --json");
        let keys = v
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<String>>();
        (keys, v["entry"].clone())
    };
    let (issue_keys, issue_entry) = echo(ok(&db, &["issue", "log", "CLI-1", "second"]));
    let (milestone_keys, milestone_entry) = echo(ok(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "second",
        ],
    ));
    assert_eq!(issue_keys, ["key", "timestamp"]);
    assert_eq!(
        milestone_keys,
        ["entry", "milestone", "project", "timestamp"]
    );
    assert!(issue_entry.is_null());
    assert_eq!(milestone_entry, "second");
}

#[test]
fn output_shape_is_the_documented_json_and_human_line() {
    let db = seeded("shape", "## Spec\n\nbody\n");

    // Piped stdout defaults to JSON, which is what the scripts read.
    let out = ok(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "[cliban-flow] milestone start CLI: started",
        ],
    );
    let v: serde_json::Value = serde_json::from_str(out.trim()).expect("log --json");
    assert_eq!(v["entry"], "[cliban-flow] milestone start CLI: started");
    assert_eq!(v["milestone"], "Deterministic integration");
    assert_eq!(v["project"], "CLI");
    assert!(
        v["timestamp"].as_str().is_some_and(|t| t.contains('T')),
        "timestamp missing: {v}"
    );

    let human = ok(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "--table",
            "second",
        ],
    );
    assert_eq!(
        human.trim(),
        "logged on milestone Deterministic integration in CLI: second"
    );
}

#[test]
fn a_leading_hyphen_is_a_message_not_a_flag() {
    let db = seeded("hyphen", "## Spec\n\nbody\n");
    ok(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "- already a bullet",
        ],
    );
    let got = entries(&db);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].ends_with(" — - already a bullet"), "{got:?}");
}

/// The load-bearing one. Several agents record against one milestone at the
/// same time, and `board_record()` in `plugin-flow/scripts/cliban-flow`
/// discards a failed append with a warning — so a writer that is *rejected*
/// leaves the board just as wrong as one that is *clobbered*, and only exit
/// codes distinguish the two.
///
/// Hence both halves of the assertion. Checking only "no entry was lost" would
/// pass on a run where eleven of twelve writers died before writing anything,
/// which is exactly what the deferred transaction `issue log` uses does:
/// against that version this test fails with `database is locked` exit-3
/// children and a short section.
#[test]
fn concurrent_appends_all_survive() {
    const WRITERS: usize = 12;

    let db = seeded("concurrent", "## Spec\n\nbody\n");
    let handles: Vec<_> = (0..WRITERS)
        .map(|i| {
            let db = db.clone();
            std::thread::spawn(move || {
                let out = cmd(&db)
                    .args([
                        "milestone",
                        "log",
                        "Deterministic integration",
                        "-p",
                        "CLI",
                        &format!("writer {i}"),
                    ])
                    .output()
                    .expect("run cliban");
                (
                    out.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&out.stderr).to_string(),
                )
            })
        })
        .collect();

    let results: Vec<(i32, String)> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let failed: Vec<&(i32, String)> = results.iter().filter(|(code, _)| *code != 0).collect();
    assert!(
        failed.is_empty(),
        "{} of {WRITERS} concurrent writers were rejected: {failed:?}",
        failed.len()
    );

    let got = entries(&db);
    assert_eq!(got.len(), WRITERS, "entries lost or duplicated: {got:?}");
    for i in 0..WRITERS {
        assert!(
            got.iter().any(|e| e.ends_with(&format!(" — writer {i}"))),
            "writer {i}'s entry is missing from {got:?}"
        );
    }
    assert_eq!(
        description(&db).matches("## Activity Log").count(),
        1,
        "concurrent writers must share one section"
    );
    // The description they were all splicing into is still intact.
    assert!(description(&db).starts_with("## Spec\n\nbody\n"));
}

#[test]
fn unknown_milestone_and_unknown_project_exit_non_zero() {
    let db = seeded("missing", "## Spec\n\nbody\n");

    let r = run(
        &db,
        &["milestone", "log", "No Such Milestone", "-p", "CLI", "x"],
    );
    assert_ne!(
        r.code, 0,
        "unknown milestone must not succeed: {}",
        r.stdout
    );

    let r = run(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "NOPE",
            "x",
        ],
    );
    assert_ne!(r.code, 0, "unknown project must not succeed: {}", r.stdout);

    // Neither attempt wrote anything.
    let desc = description(&db);
    assert!(!desc.contains("## Activity Log"), "{desc:?}");
}

#[test]
fn an_empty_message_is_refused() {
    let db = seeded("blank", "## Spec\n\nbody\n");
    let r = run(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "",
        ],
    );
    assert_ne!(r.code, 0, "an empty entry is not a log line");
    assert!(
        r.stderr.contains("message required"),
        "expected the same refusal `issue log` gives: {:?}",
        r.stderr
    );
}

#[test]
fn the_message_can_come_from_a_pipe_or_a_file() {
    use std::io::Write;
    use std::process::Stdio;

    let db = seeded("stdin", "## Spec\n\nbody\n");

    // A bare pipe with no positional: stdin IS the message.
    let mut child = cmd(&db)
        .args(["milestone", "log", "Deterministic integration", "-p", "CLI"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"piped entry\n")
        .unwrap();
    assert!(child.wait().expect("wait").success());

    // `--message-file -` reads the pipe explicitly.
    let mut child = cmd(&db)
        .args([
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "--message-file",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"filed entry\n")
        .unwrap();
    assert!(child.wait().expect("wait").success());

    // An actual file on disk — the one arm of the shared resolver that no
    // other test in this crate exercises, and the arm the flow scripts use for
    // any entry too long or too quote-heavy to pass as an argument.
    let path = std::env::temp_dir().join(format!("cliban_mlog_entry_{}.txt", std::process::id()));
    std::fs::write(&path, "entry from a real file\n").expect("write fixture");
    ok(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "--message-file",
            path.to_str().unwrap(),
        ],
    );
    std::fs::remove_file(&path).ok();

    // A path that does not exist is refused, not silently logged as its name.
    let r = run(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "--message-file",
            "/nonexistent/entry.txt",
        ],
    );
    assert_ne!(r.code, 0, "a missing --message-file must not succeed");

    let got = entries(&db);
    assert_eq!(got.len(), 3, "{got:?}");
    assert!(got[0].ends_with(" — piped entry"), "{got:?}");
    assert!(got[1].ends_with(" — filed entry"), "{got:?}");
    assert!(got[2].ends_with(" — entry from a real file"), "{got:?}");

    // Both spellings at once is a refusal, not a silent winner.
    let r = run(
        &db,
        &[
            "milestone",
            "log",
            "Deterministic integration",
            "-p",
            "CLI",
            "positional",
            "--message-file",
            "/dev/null",
        ],
    );
    assert_ne!(r.code, 0);
    assert_eq!(entries(&db).len(), 3, "the refusal must not have written");
}
