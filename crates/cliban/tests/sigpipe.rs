//! SIGPIPE end-to-end: `cliban <anything> | head` must never panic.
//!
//! Any command whose output exceeds the pipe buffer after the reader exits
//! used to die with `failed printing to stdout: Broken pipe` (a panic, exit
//! 101), because Rust ignores SIGPIPE by default and `println!` panics on
//! EPIPE. The fix restores SIGPIPE's default disposition in main, so the
//! process dies quietly the way cat/grep/git do.
//!
//! The reproduction is deterministic: we seed a board with well over 64 KiB
//! (the Linux pipe buffer) of activity, spawn the CLI with a piped stdout,
//! and close the read end without draining it. The child fills the pipe
//! buffer, blocks, and the moment the reader closes its next write raises
//! EPIPE — no timing luck involved.

#![cfg(unix)]

use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

fn bin() -> String {
    env!("CARGO_BIN_EXE_cliban").to_string()
}

fn tmp_db(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("cliban_sigpipe_{tag}_{nanos}.db"));
    path.to_string_lossy().to_string()
}

/// Base command with a clean env: never inherit the developer's CLIBAN_DB,
/// nor the ambient Claude session which would auto-attribute entries.
fn cmd(db: &str) -> Command {
    let mut c = Command::new(bin());
    c.arg("--db")
        .arg(db)
        .env_remove("CLIBAN_DB")
        .env_remove("XDG_DATA_HOME")
        .env_remove("CLIBAN_ACTOR")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    c
}

fn ok(db: &str, args: &[&str]) {
    let out = cmd(db).args(args).output().expect("run cliban");
    assert!(
        out.status.success(),
        "`cliban {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Seed a board whose `activity --json` output comfortably exceeds the 64 KiB
/// pipe buffer: a handful of log entries with long bodies.
fn seed_busy_board(db: &str) {
    ok(db, &["project", "add", "SP", "Sigpipe"]);
    ok(db, &["issue", "add", "noisy issue", "--project", "SP"]);
    let long_line = "x".repeat(8 * 1024);
    for _ in 0..12 {
        ok(db, &["issue", "log", "SP-1", &long_line]);
    }
}

/// Run `args`, close the pipe's read end without draining, and return the
/// process's exit status plus captured stderr.
fn run_into_closed_pipe(db: &str, args: &[&str]) -> (std::process::ExitStatus, String) {
    let mut child = cmd(db)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cliban");
    // Close the read end immediately. Output > 64 KiB guarantees the child
    // cannot fit everything in the pipe buffer, so a write must hit EPIPE.
    drop(child.stdout.take());
    let mut stderr = String::new();
    use std::io::Read;
    child
        .stderr
        .take()
        .expect("stderr piped")
        .read_to_string(&mut stderr)
        .expect("read stderr");
    let status = child.wait().expect("wait for cliban");
    (status, stderr)
}

#[test]
fn activity_piped_to_closed_reader_does_not_panic() {
    let db = tmp_db("activity");
    seed_busy_board(&db);

    let (status, stderr) = run_into_closed_pipe(&db, &["activity", "--json", "--limit", "0"]);

    assert!(
        !stderr.contains("Broken pipe") && !stderr.contains("panicked"),
        "cliban panicked on EPIPE:\n{stderr}"
    );
    // Dying of SIGPIPE (like cat/grep) or finishing cleanly are both fine;
    // a panic's exit code 101 is not.
    assert!(
        status.signal() == Some(libc_sigpipe()) || status.code() == Some(0),
        "expected SIGPIPE death or clean exit, got {status:?}"
    );
}

#[test]
fn issue_show_piped_to_closed_reader_does_not_panic() {
    // The fix must cover every command, not just activity: `issue show` of a
    // large description is another single-write path past the pipe buffer.
    let db = tmp_db("show");
    ok(&db, &["project", "add", "SP", "Sigpipe"]);
    ok(&db, &["issue", "add", "big", "--project", "SP"]);
    let big_body = format!("## Spec\n\n{}", "y".repeat(128 * 1024));
    let out = cmd(&db)
        .args(["issue", "edit", "SP-1", "--description-file", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(big_body.as_bytes())?;
            c.wait_with_output()
        })
        .expect("seed big description");
    assert!(
        out.status.success(),
        "seeding big description failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (status, stderr) = run_into_closed_pipe(&db, &["issue", "show", "SP-1", "--json"]);

    assert!(
        !stderr.contains("Broken pipe") && !stderr.contains("panicked"),
        "cliban panicked on EPIPE:\n{stderr}"
    );
    assert!(
        status.signal() == Some(libc_sigpipe()) || status.code() == Some(0),
        "expected SIGPIPE death or clean exit, got {status:?}"
    );
}

/// SIGPIPE's number, spelled out so the test crate needs no libc dependency.
/// 13 on every unix cliban targets (Linux, macOS, BSDs).
fn libc_sigpipe() -> i32 {
    13
}
