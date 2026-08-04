//! Unix spares: the issue is cliban's default noun.
//!
//! Two hidden layers over the canonical `issue` surface:
//!
//!   * top-level `cliban ls|mv|rm|show|log|tick|cat` forward to
//!     `cliban issue <verb>` at the clap level — identical args, flags, output
//!     bytes, and exit codes;
//!   * GitHub reflexes on the issue namespace — `close` → `mv done`,
//!     `reopen` → `mv in-progress`, `comment` → `log`, `delete` → rm
//!     behavior — each confirmation stating the canonical form once.
//!
//! None of them appear in any `--help`; the one genuinely new canonical
//! command, `issue cat` (raw description, verbatim, never formatted), does.

use std::io::Write;
use std::process::{Command, Stdio};

fn bin() -> String {
    env!("CARGO_BIN_EXE_cliban").to_string()
}

fn tmp_db(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("cliban_spares_{tag}_{nanos}.db"))
        .to_string_lossy()
        .to_string()
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

fn base_cmd(db: &str, args: &[&str]) -> Command {
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
    cmd
}

/// Spawn with null stdin (EOF immediately) and piped stdout/stderr, so the
/// stdin-fallback paths never hang on the harness's own stdin.
fn run(db: &str, args: &[&str]) -> Run {
    let out = base_cmd(db, args)
        .stdin(Stdio::null())
        .output()
        .expect("run cliban");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// Run with `content` piped into stdin.
fn run_piped_stdin(db: &str, args: &[&str], content: &str) -> Run {
    let mut child = base_cmd(db, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cliban");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(content.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait cliban");
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

const DESC: &str =
    "## Spec\n\nspare verbs\n\n## Plan\n\n### Task 1: t\n\n- [ ] **Step 1: s** — do\n\n## Notes\n";

/// A board with one project and one issue carrying a full description.
fn seeded(tag: &str) -> String {
    let db = tmp_db(tag);
    ok(&db, &["project", "add", "UX", "Unix Spares"]);
    ok(
        &db,
        &[
            "issue",
            "add",
            "alpha",
            "--project",
            "UX",
            "--description",
            DESC,
        ],
    );
    db
}

fn show_json(db: &str, key: &str) -> serde_json::Value {
    serde_json::from_str(&ok(db, &["issue", "show", key, "--json"])).expect("issue json")
}

fn assert_same(canonical: &Run, alias: &Run, what: &str) {
    assert_eq!(alias.stdout, canonical.stdout, "{what}: stdout diverged");
    assert_eq!(alias.stderr, canonical.stderr, "{what}: stderr diverged");
    assert_eq!(alias.code, canonical.code, "{what}: exit code diverged");
}

// --- layer a: hidden top-level verbs forward byte-identically ----------------

#[test]
fn top_level_reads_are_byte_identical_to_issue_reads() {
    let db = seeded("reads");
    for (canonical, alias) in [
        (vec!["issue", "ls", "--json"], vec!["ls", "--json"]),
        (
            vec![
                "issue",
                "ls",
                "--project",
                "UX",
                "--status",
                "backlog",
                "--table",
            ],
            vec!["ls", "--project", "UX", "--status", "backlog", "--table"],
        ),
        (
            vec!["issue", "show", "UX-1", "--json"],
            vec!["show", "UX-1", "--json"],
        ),
        (
            vec!["issue", "cat", "UX-1", "--section", "spec"],
            vec!["cat", "UX-1", "--section", "spec"],
        ),
        (vec!["issue", "cat", "UX-1"], vec!["cat", "UX-1"]),
    ] {
        let c = run(&db, &canonical);
        let a = run(&db, &alias);
        assert_eq!(c.code, 0, "canonical {canonical:?} failed: {}", c.stderr);
        assert_same(&c, &a, &alias.join(" "));
    }
}

#[test]
fn top_level_mutations_print_the_exact_canonical_confirmations() {
    let db = seeded("mut");
    // mv
    let r = run(&db, &["mv", "UX-1", "in-progress", "--table"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "moved UX-1: backlog → in-progress\n");
    // retry-safe noop comes through the same handler (CLI-49)
    let r = run(&db, &["mv", "UX-1", "in-progress", "--table"]);
    assert_eq!(r.stdout, "UX-1 already in-progress (nothing to do)\n");
    // tick
    let r = run(
        &db,
        &["tick", "UX-1", "--task", "1", "--step", "1", "--table"],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "ticked UX-1 Task 1 Step 1\n");
    // log
    let r = run(&db, &["log", "UX-1", "spare says hi", "--table"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "logged on UX-1: spare says hi\n");
    // rm
    let r = run(&db, &["rm", "UX-1"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "archived UX-1 — cliban archives instead of deleting (undo: cliban issue unarchive UX-1)\n"
    );
    assert_eq!(show_json(&db, "UX-1")["archived"], serde_json::json!(true));
}

#[test]
fn top_level_error_paths_are_byte_identical_to_issue_errors() {
    let db = seeded("errs");
    for (canonical, alias) in [
        // invalid status (exit 3)
        (
            vec!["issue", "mv", "UX-1", "bogus"],
            vec!["mv", "UX-1", "bogus"],
        ),
        // missing issue (exit 1)
        (
            vec!["issue", "show", "UX-99", "--json"],
            vec!["show", "UX-99", "--json"],
        ),
        (vec!["issue", "cat", "UX-99"], vec!["cat", "UX-99"]),
        // no message anywhere (exit 2; null stdin = empty pipe)
        (vec!["issue", "log", "UX-1"], vec!["log", "UX-1"]),
        // step out of range (exit 2)
        (
            vec!["issue", "tick", "UX-1", "--task", "1", "--step", "9"],
            vec!["tick", "UX-1", "--task", "1", "--step", "9"],
        ),
        // malformed key (exit 2)
        (vec!["issue", "rm", "nope"], vec!["rm", "nope"]),
    ] {
        let c = run(&db, &canonical);
        let a = run(&db, &alias);
        assert_ne!(c.code, 0, "expected {canonical:?} to fail");
        assert_same(&c, &a, &alias.join(" "));
    }
}

#[test]
fn top_level_rm_matches_issue_rm_byte_for_byte() {
    // The message is deterministic, so two identically-seeded boards compare.
    let a_db = seeded("rm_canon");
    let b_db = seeded("rm_spare");
    let c = run(&a_db, &["issue", "rm", "UX-1"]);
    let a = run(&b_db, &["rm", "UX-1"]);
    assert_same(&c, &a, "rm");
}

// --- issue cat: raw description, verbatim, never formatted -------------------

#[test]
fn cat_dumps_the_raw_description_verbatim() {
    let db = seeded("cat");
    // Byte-exact: exactly what was stored, no header, no trailing decoration.
    assert_eq!(ok(&db, &["issue", "cat", "UX-1"]), DESC);
    assert_eq!(ok(&db, &["cat", "UX-1"]), DESC);
}

#[test]
fn cat_stays_raw_when_piped() {
    // Piped stdout is the JSON default everywhere else (CLI-48); cat is the
    // deliberate exception — its whole point is unformatted bytes.
    let db = seeded("cat_pipe");
    let out = ok(&db, &["issue", "cat", "UX-1"]);
    assert!(
        !out.trim_start().starts_with('{'),
        "cat must never emit JSON: {out}"
    );
    assert!(out.contains("## Spec"), "raw markdown expected: {out}");
}

// --- layer b: reflex aliases teach the canonical form ------------------------

#[test]
fn close_moves_to_done_and_teaches_mv() {
    let db = seeded("close");
    let r = run(&db, &["issue", "close", "UX-1", "--table"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "closed UX-1 (mv done): backlog → done\n");
    assert_eq!(show_json(&db, "UX-1")["status"], serde_json::json!("done"));
    // Retry is the CLI-49 noop, still teaching.
    let r = run(&db, &["issue", "close", "UX-1", "--table"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "closed UX-1 (mv done): already done (nothing to do)\n"
    );
}

#[test]
fn close_output_is_the_mv_output_modulo_the_teaching_prefix() {
    let a_db = seeded("close_canon");
    let b_db = seeded("close_alias");
    let canonical = run(&a_db, &["issue", "mv", "UX-1", "done", "--table"]);
    let alias = run(&b_db, &["issue", "close", "UX-1", "--table"]);
    assert_eq!(
        alias.stdout,
        canonical
            .stdout
            .replace("moved UX-1:", "closed UX-1 (mv done):"),
        "close must be mv done wearing a teaching prefix"
    );
    assert_eq!(alias.code, canonical.code);
}

#[test]
fn close_json_echo_keeps_the_mv_shape_and_adds_canonical() {
    let db = seeded("close_json");
    let v: serde_json::Value =
        serde_json::from_str(&ok(&db, &["issue", "close", "UX-1", "--json"])).expect("json echo");
    assert_eq!(v["status"], serde_json::json!("done"));
    assert_eq!(v["key"], serde_json::json!("UX-1"));
    assert_eq!(v["canonical"], serde_json::json!("issue mv done"));
    // The echo is the lean mutation shape (plus the teach marker); updated_at
    // keeps CAS precision so a piped caller can chain without a re-read.
    for field in ["title", "priority", "updated_at"] {
        assert!(v.get(field).is_some(), "echo lost the {field} field: {v}");
    }
    assert!(v.get("description").is_none(), "echoes carry no body: {v}");
    // Noop retry keeps the CLI-49 marker alongside the teach marker.
    let v: serde_json::Value =
        serde_json::from_str(&ok(&db, &["issue", "close", "UX-1", "--json"])).expect("json echo");
    assert_eq!(v["noop"], serde_json::json!(true));
    assert_eq!(v["canonical"], serde_json::json!("issue mv done"));
}

#[test]
fn close_note_lands_on_the_timeline_like_mv_note() {
    let db = seeded("close_note");
    ok(
        &db,
        &[
            "issue",
            "close",
            "UX-1",
            "--note",
            "shipped in v2",
            "--table",
        ],
    );
    let activity = ok(&db, &["activity", "--issue", "UX-1", "--table"]);
    assert!(activity.contains("shipped in v2"), "activity: {activity}");
}

#[test]
fn reopen_moves_to_in_progress_and_teaches_mv() {
    let db = seeded("reopen");
    ok(&db, &["issue", "mv", "UX-1", "done", "--table"]);
    let r = run(&db, &["issue", "reopen", "UX-1", "--table"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "reopened UX-1 (mv in-progress): done → in-progress\n"
    );
    assert_eq!(
        show_json(&db, "UX-1")["status"],
        serde_json::json!("in-progress")
    );
    let v: serde_json::Value =
        serde_json::from_str(&ok(&db, &["issue", "reopen", "UX-1", "--json"])).expect("json echo");
    assert_eq!(v["canonical"], serde_json::json!("issue mv in-progress"));
    assert_eq!(v["noop"], serde_json::json!(true));
}

#[test]
fn comment_is_log_with_a_teaching_line() {
    let db = seeded("comment");
    let r = run(&db, &["issue", "comment", "UX-1", "looks right", "--table"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "commented UX-1 (log): looks right\n");
    let activity = ok(&db, &["issue", "cat", "UX-1", "--section", "activity"]);
    assert!(activity.contains("looks right"), "activity: {activity}");
}

#[test]
fn comment_inherits_the_stdin_fallback() {
    // CLI-50's pipe-is-the-message contract comes through the same handler.
    let db = seeded("comment_pipe");
    let r = run_piped_stdin(&db, &["issue", "comment", "UX-1"], "from the pipe\n");
    assert_eq!(r.code, 0, "{}", r.stderr);
    let activity = ok(&db, &["issue", "cat", "UX-1", "--section", "activity"]);
    assert!(activity.contains("from the pipe"), "activity: {activity}");
    // Empty pipe keeps log's fast validation error, byte-identically.
    let c = run(&db, &["issue", "log", "UX-1"]);
    let a = run(&db, &["issue", "comment", "UX-1"]);
    assert_eq!(a.code, 2, "empty comment must stay exit 2: {}", a.stderr);
    assert_eq!(a.stderr, c.stderr, "comment must fail exactly like log");
}

#[test]
fn comment_json_is_the_log_shape_plus_canonical() {
    let db = seeded("comment_json");
    let mut c: serde_json::Value =
        serde_json::from_str(&ok(&db, &["issue", "log", "UX-1", "same words", "--json"]))
            .expect("log json");
    let mut a: serde_json::Value = serde_json::from_str(&ok(
        &db,
        &["issue", "comment", "UX-1", "same words", "--json"],
    ))
    .expect("comment json");
    assert_eq!(a["canonical"], serde_json::json!("issue log"));
    // Timestamps differ between the two runs; everything else must not.
    let (ca, aa) = (c.as_object_mut().unwrap(), a.as_object_mut().unwrap());
    ca.remove("timestamp");
    aa.remove("timestamp");
    aa.remove("canonical");
    assert_eq!(aa, ca, "comment JSON must be the log JSON plus canonical");
}

#[test]
fn delete_archives_and_teaches_that_nothing_was_deleted() {
    let db = seeded("delete");
    let r = run(&db, &["issue", "delete", "UX-1"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "archived UX-1 (archive) — cliban archives instead of deleting \
         (undo: cliban issue unarchive UX-1)\n"
    );
    // Archived, not destroyed — and reversible.
    let v = show_json(&db, "UX-1");
    assert_eq!(v["archived"], serde_json::json!(true));
    assert_eq!(v["title"], serde_json::json!("alpha"));
    ok(&db, &["issue", "unarchive", "UX-1", "--table"]);
    assert_eq!(show_json(&db, "UX-1")["archived"], serde_json::json!(false));
    // Desired state = success: deleting again is the same success (CLI-49).
    ok(&db, &["issue", "delete", "UX-1"]);
    let r = run(&db, &["issue", "delete", "UX-1"]);
    assert_eq!(r.code, 0, "retry must stay success: {}", r.stderr);
}

#[test]
fn delete_output_is_the_rm_output_modulo_the_teaching_prefix() {
    let a_db = seeded("delete_canon");
    let b_db = seeded("delete_alias");
    let canonical = run(&a_db, &["issue", "rm", "UX-1"]);
    let alias = run(&b_db, &["issue", "delete", "UX-1"]);
    assert_eq!(
        alias.stdout,
        canonical
            .stdout
            .replace("archived UX-1 —", "archived UX-1 (archive) —"),
        "delete must be rm wearing a teaching prefix"
    );
    assert_eq!(alias.code, canonical.code);
}

// --- none of it is advertised ------------------------------------------------

/// The listed command names under a `--help`'s "Commands:" heading.
fn help_commands(db: &str, args: &[&str]) -> Vec<String> {
    let (help, _, _) = {
        let r = run(db, args);
        (r.stdout, r.stderr, r.code)
    };
    let commands: Vec<String> = help
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    assert!(!commands.is_empty(), "could not parse {args:?}: {help}");
    commands
}

#[test]
fn help_advertises_no_spare_verbs() {
    let db = seeded("help_top");
    let commands = help_commands(&db, &["--help"]);
    for spare in ["ls", "mv", "rm", "show", "log", "tick", "cat"] {
        assert!(
            !commands.iter().any(|c| c == spare),
            "`cliban --help` advertises the spare `{spare}`: {commands:?}"
        );
    }
    assert!(
        commands.iter().any(|c| c == "issue"),
        "sanity: parsed the top-level command list: {commands:?}"
    );
}

#[test]
fn issue_help_advertises_no_reflex_aliases_but_does_advertise_cat() {
    let db = seeded("help_issue");
    let commands = help_commands(&db, &["issue", "--help"]);
    for alias in ["close", "reopen", "comment", "delete", "rm"] {
        assert!(
            !commands.iter().any(|c| c == alias),
            "`cliban issue --help` advertises `{alias}`: {commands:?}"
        );
    }
    assert!(
        commands.iter().any(|c| c == "cat"),
        "`issue cat` is canonical and must be discoverable: {commands:?}"
    );
}

#[test]
fn spares_still_answer_their_own_help() {
    // Hidden ≠ broken: `cliban mv --help` must answer (exit 0), because an
    // agent that guessed the verb will guess `--help` next.
    let db = seeded("help_spares");
    for args in [
        vec!["ls", "--help"],
        vec!["mv", "--help"],
        vec!["rm", "--help"],
        vec!["show", "--help"],
        vec!["log", "--help"],
        vec!["tick", "--help"],
        vec!["cat", "--help"],
        vec!["issue", "close", "--help"],
        vec!["issue", "reopen", "--help"],
        vec!["issue", "comment", "--help"],
        vec!["issue", "delete", "--help"],
    ] {
        let r = run(&db, &args);
        assert_eq!(r.code, 0, "`cliban {}` must answer", args.join(" "));
    }
}
