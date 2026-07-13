//! Golden parity test harness.
//!
//! Runs a battery of commands through BOTH the Rust `cliban` binary and the Go
//! `cliban` oracle binary, each against its OWN freshly-seeded temp DB, and
//! asserts the outputs match after normalizing volatile fields (timestamps).
//!
//! The two binaries use different DB schemas (Rust: `issues`/`projects`; Go:
//! `issue`/`project`) and CANNOT share a DB file. So each test seeds each
//! binary's DB by replaying the SAME sequence of write commands through THAT
//! binary, then runs the assertion command through both and compares.
//!
//! If the Go binary is missing, tests SKIP gracefully (print + return).

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

fn go_bin() -> String {
    std::env::var("CLIBAN_GO_BIN").unwrap_or_else(|_| "/home/alex/dev/cliban/cliban".into())
}

fn rust_bin() -> String {
    env!("CARGO_BIN_EXE_cliban").to_string()
}

fn go_available() -> bool {
    std::path::Path::new(&go_bin()).exists()
}

#[derive(Debug)]
struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn tmp_db(tag: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("cliban_golden_{}_{}_{}.db", tag, nanos, n));
    let s = path.to_string_lossy().to_string();
    // Ensure a clean slate.
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", s, suffix));
    }
    s
}

/// Run a command through `bin` against `db` with a CLEAN environment (we do NOT
/// inherit the developer's CLIBAN_DB / XDG_DATA_HOME). Optionally pass stdin and
/// extra env vars.
fn run_env(
    bin: &str,
    db: &str,
    args: &[&str],
    stdin: Option<&str>,
    extra_env: &[(&str, &str)],
) -> Run {
    let mut cmd = Command::new(bin);
    // env_clear gives us a clean env; we then set only what we need.
    cmd.env_clear();
    cmd.env("CLIBAN_DB", db);
    // A few binaries need HOME/PATH-ish bits to be sane; keep PATH for safety.
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });

    let mut child = cmd.spawn().expect("spawn binary");
    if let Some(input) = stdin {
        child
            .stdin
            .as_mut()
            .expect("stdin pipe")
            .write_all(input.as_bytes())
            .expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait for binary");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn run(bin: &str, db: &str, args: &[&str], stdin: Option<&str>) -> Run {
    run_env(bin, db, args, stdin, &[])
}

/// Replace volatile timestamp fields with a stable placeholder so the two
/// binaries' outputs can be compared. Scores and positions are NOT volatile and
/// are deliberately left untouched.
fn normalize(s: &str) -> String {
    // Full RFC3339 with optional fractional seconds: nanosecond (Go) and
    // microsecond (Rust) both collapse to <TS>.
    let full = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z").unwrap();
    // Minute-form RFC3339 used in activity-log entries: `2026-06-19T15:48Z`.
    let minute = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}Z").unwrap();
    let out = full.replace_all(s, "<TS>");
    let out = minute.replace_all(&out, "<TS>");
    out.into_owned()
}

/// Seed both DBs with the same script, then run `cmd` on both and assert parity.
fn assert_parity_env(
    seed: &[&[&str]],
    cmd: &[&str],
    stdin: Option<&str>,
    extra_env: &[(&str, &str)],
) {
    let gdb = tmp_db("go");
    let rdb = tmp_db("rust");
    for s in seed {
        run(&go_bin(), &gdb, s, None);
        run(&rust_bin(), &rdb, s, None);
    }
    let g = run_env(&go_bin(), &gdb, cmd, stdin, extra_env);
    let r = run_env(&rust_bin(), &rdb, cmd, stdin, extra_env);
    assert_eq!(
        normalize(&g.stdout),
        normalize(&r.stdout),
        "stdout mismatch for {:?}\nGO:\n{}\nRUST:\n{}",
        cmd,
        g.stdout,
        r.stdout
    );
    assert_eq!(
        g.code, r.code,
        "exit code mismatch for {:?}\nGO code={} stderr={}\nRUST code={} stderr={}",
        cmd, g.code, g.stderr, r.code, r.stderr
    );
    assert_eq!(
        normalize(&g.stderr),
        normalize(&r.stderr),
        "stderr mismatch for {:?}\nGO:\n{}\nRUST:\n{}",
        cmd,
        g.stderr,
        r.stderr
    );
}

fn assert_parity(seed: &[&[&str]], cmd: &[&str], stdin: Option<&str>) {
    assert_parity_env(seed, cmd, stdin, &[]);
}

/// The shared, realistic seed dataset replayed through each binary.
fn base_seed() -> Vec<Vec<&'static str>> {
    vec![
        vec![
            "project",
            "add",
            "CLI",
            "--name",
            "CLI Project",
            "--description",
            "the cli",
        ],
        vec![
            "issue",
            "add",
            "--project",
            "CLI",
            "--title",
            "First issue",
            "--priority",
            "high",
            "--label",
            "bug",
        ],
        vec![
            "issue",
            "add",
            "--project",
            "CLI",
            "--title",
            "Second thing",
            "--label",
            "feature",
            "--label",
            "bug",
        ],
        vec![
            "milestone",
            "add",
            "--project",
            "CLI",
            "--name",
            "v1",
            "--description",
            "milestone one",
            "--target",
            "2026-12-31",
        ],
        vec![
            "issue",
            "add",
            "--project",
            "CLI",
            "--title",
            "Third",
            "--milestone",
            "v1",
            "--blocks",
            "CLI-1",
        ],
        vec![
            "issue",
            "add",
            "--project",
            "CLI",
            "--title",
            "Fourth",
            "--parent",
            "CLI-1",
        ],
    ]
}

macro_rules! skip_if_no_go {
    () => {
        if !go_available() {
            eprintln!(
                "SKIP: Go oracle binary not found at {} (set CLIBAN_GO_BIN)",
                go_bin()
            );
            return;
        }
    };
}

/// Convenience: run a list of `cmd`s against the base seed.
fn parity_all(seed: &[Vec<&'static str>], cmds: &[&[&str]]) {
    for cmd in cmds {
        let s: Vec<&[&str]> = seed.iter().map(|v| v.as_slice()).collect();
        assert_parity(&s, cmd, None);
    }
}

#[test]
fn test_project_parity() {
    skip_if_no_go!();
    let seed = base_seed();
    parity_all(
        &seed,
        &[
            &["project", "ls", "--json"],
            &["project", "ls"],
            &["project", "show", "CLI", "--json"],
            &["project", "show", "CLI"],
            &["project", "add", "NEW", "--name", "New", "--json"],
        ],
    );
}

#[test]
fn test_issue_read_parity() {
    skip_if_no_go!();
    let seed = base_seed();
    parity_all(
        &seed,
        &[
            &["issue", "ls", "--json"],
            &["issue", "ls"],
            &["issue", "ls", "--status", "backlog", "--json"],
            &["issue", "ls", "--priority", "high", "--json"],
            &[
                "issue",
                "ls",
                "--milestone",
                "v1",
                "--project",
                "CLI",
                "--json",
            ],
            &["issue", "ls", "--label", "bug", "--json"],
            &["issue", "ls", "--no-subs", "--json"],
            &["issue", "ls", "--sort", "priority", "--json"],
            &["issue", "show", "CLI-1", "--json"],
            &["issue", "show", "CLI-1"],
            &["issue", "show", "CLI-3", "--json"], // has a `blocks` relation
        ],
    );

    // `issue current` resolves the issue from the current git branch; force a
    // deterministic branch on BOTH binaries via the override env var.
    let s: Vec<&[&str]> = seed.iter().map(|v| v.as_slice()).collect();
    assert_parity_env(
        &s,
        &["issue", "current", "--json"],
        None,
        &[("CLIBAN_CURRENT_BRANCH_OVERRIDE", "cli-1-first-issue")],
    );
}

#[test]
fn test_issue_mutation_parity() {
    skip_if_no_go!();
    let seed = base_seed();
    parity_all(
        &seed,
        &[
            &[
                "issue",
                "add",
                "--project",
                "CLI",
                "--title",
                "X",
                "--priority",
                "urgent",
                "--label",
                "new",
                "--json",
            ],
            &["issue", "edit", "CLI-1", "--priority", "urgent", "--json"],
            &["issue", "edit", "CLI-1", "--title", "Renamed", "--json"],
            &["issue", "mv", "CLI-1", "in-progress"], // no output; empty + exit 0
            &["issue", "blocked", "--json"],
            &["issue", "archive-done", "--auto", "--json"],
        ],
    );
}

#[test]
fn test_workflow_parity() {
    skip_if_no_go!();
    // A description containing a Plan and where activity entries will land.
    // NOTE: real newlines here (Rust interprets \n), NOT shell-literal "\\n".
    let desc = "## Plan\n\n### Task 1: a\n\n- [ ] Step 1\n- [ ] Step 2\n";
    let p_add: &[&str] = &[
        "issue",
        "add",
        "--project",
        "CLI",
        "--title",
        "P",
        "--description",
        desc,
    ];
    let base = base_seed();
    let mut seed_slices: Vec<&[&str]> = base.iter().map(|v| v.as_slice()).collect();
    seed_slices.push(p_add);
    // The new issue is CLI-5 (4 issues created in base_seed, P is the 5th).
    let key = "CLI-5";

    // tick a step, then confirm the plan-section mutation matches.
    assert_parity(
        &seed_slices,
        &["issue", "tick", key, "--task", "1", "--step", "1", "--json"],
        None,
    );

    // Build a seed that includes the tick, then read the plan section.
    let tick: &[&str] = &["issue", "tick", key, "--task", "1", "--step", "1"];
    let mut seed_after_tick = seed_slices.clone();
    seed_after_tick.push(tick);
    assert_parity(
        &seed_after_tick,
        &["issue", "show", key, "--section", "plan"],
        None,
    );

    // log an activity entry, then confirm the activity section matches.
    assert_parity(
        &seed_slices,
        &["issue", "log", key, "did work", "--json"],
        None,
    );
    let log: &[&str] = &["issue", "log", key, "did work"];
    let mut seed_after_log = seed_slices.clone();
    seed_after_log.push(log);
    assert_parity(
        &seed_after_log,
        &["issue", "show", key, "--section", "activity"],
        None,
    );
}

#[test]
fn test_milestone_label_parity() {
    skip_if_no_go!();
    let seed = base_seed();
    parity_all(
        &seed,
        &[
            &["milestone", "ls", "--project", "CLI", "--json"],
            &["milestone", "show", "v1", "--project", "CLI", "--json"],
            &[
                "milestone",
                "show",
                "v1",
                "--project",
                "CLI",
                "--json",
                "--with-issues",
            ],
            &["milestone", "show", "v1", "--project", "CLI"],
            &["label", "ls", "--project", "CLI", "--json"],
            &["label", "ls", "--project", "CLI"],
        ],
    );
}

#[test]
fn test_search_parity() {
    skip_if_no_go!();
    let seed = base_seed();
    // Scores must match EXACTLY — normalize() deliberately does not redact score.
    parity_all(
        &seed,
        &[
            &["issue", "ls", "--search", "thing", "--json"],
            &["fff", "thing"],
        ],
    );
}

#[test]
fn test_error_parity() {
    skip_if_no_go!();
    let seed = base_seed();
    let s: Vec<&[&str]> = seed.iter().map(|v| v.as_slice()).collect();
    // not found -> exit 1
    assert_parity(&s, &["issue", "show", "CLI-99"], None);
    // validation error -> exit 2
    assert_parity(&s, &["issue", "add", "--project", "CLI"], None);
    // invalid status transition -> exit 3
    assert_parity(&s, &["issue", "mv", "CLI-1", "bogus"], None);
}

#[test]
fn test_stdin_parity() {
    skip_if_no_go!();
    let seed = base_seed();
    let s: Vec<&[&str]> = seed.iter().map(|v| v.as_slice()).collect();
    assert_parity(
        &s,
        &[
            "issue",
            "add",
            "--project",
            "CLI",
            "--title",
            "StdinDesc",
            "--description-file",
            "-",
            "--json",
        ],
        Some("from stdin\n"),
    );
}
