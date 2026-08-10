//! Stdin fallback for primary text inputs, end-to-end.
//!
//! When the primary text argument is absent AND stdin is piped/redirected
//! (not a TTY), `issue log`, `issue append-section`, and `project note add`
//! read the text from stdin — no `--message-file -` / `--text-file -` /
//! `--body-file -` incantation needed. Explicit arguments always win over
//! the pipe; an empty pipe keeps the clean validation error where the text
//! is required (log, append-section) and today's bare-heading note where it
//! is optional (note add); a real TTY with no argument keeps the fast error
//! and never blocks waiting for input.
//!
//! The other way in is explicit: a bare `-` on a text flag or its `--*-file`
//! sibling. That one is a sentinel rather than a payload, and every command
//! reads it the same way — the last block here is what keeps them agreeing.

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
        .join(format!("cliban_stdinfb_{tag}_{nanos}.db"))
        .to_string_lossy()
        .to_string()
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Base command with a scrubbed env. Stdin is set by each caller — that is
/// the whole point of this file.
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

/// Run with stdin wired to `/dev/null` (an empty, decidedly non-TTY pipe).
fn run_null_stdin(db: &str, args: &[&str]) -> Run {
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
    // stdin handle dropped above → EOF for the child.
    let out = child.wait_with_output().expect("wait cliban");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn ok_null(db: &str, args: &[&str]) -> String {
    let r = run_null_stdin(db, args);
    assert_eq!(
        r.code,
        0,
        "`cliban {}` failed: {}",
        args.join(" "),
        r.stderr
    );
    r.stdout
}

/// A board with one project and one issue.
fn seeded(tag: &str) -> String {
    let db = tmp_db(tag);
    ok_null(&db, &["project", "add", "SF", "Stdin Fallback"]);
    ok_null(&db, &["issue", "add", "alpha", "--project", "SF"]);
    db
}

// --- piped stdin, argument absent → the pipe IS the text ---------------------

#[test]
fn piped_log_message_comes_from_stdin() {
    let db = seeded("log_pipe");
    let r = run_piped_stdin(&db, &["issue", "log", "SF-1"], "found the frobnicator\n");
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let activity = ok_null(&db, &["issue", "cat", "SF-1", "--section", "activity"]);
    assert!(
        activity.contains("found the frobnicator"),
        "activity: {activity}"
    );
}

#[test]
fn piped_append_section_text_comes_from_stdin() {
    let db = seeded("append_pipe");
    let r = run_piped_stdin(
        &db,
        &[
            "issue",
            "append-section",
            "SF-1",
            "--section",
            "notes",
            "--create-section",
        ],
        "- a piped lesson\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let notes = ok_null(&db, &["issue", "cat", "SF-1", "--section", "notes"]);
    assert!(notes.contains("- a piped lesson"), "notes: {notes}");
}

#[test]
fn piped_note_add_body_comes_from_stdin() {
    let db = seeded("note_pipe");
    let r = run_piped_stdin(
        &db,
        &["project", "note", "add", "SF", "Piped lesson"],
        "the body arrived by pipe\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let notes = ok_null(&db, &["project", "cat", "SF", "--section", "notes"]);
    assert!(notes.contains("### Piped lesson"), "notes: {notes}");
    assert!(notes.contains("the body arrived by pipe"), "notes: {notes}");
}

// --- explicit argument always wins over the pipe -----------------------------

#[test]
fn explicit_log_message_beats_piped_stdin() {
    let db = seeded("log_arg_wins");
    let r = run_piped_stdin(
        &db,
        &["issue", "log", "SF-1", "from the arg"],
        "from the pipe\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let activity = ok_null(&db, &["issue", "cat", "SF-1", "--section", "activity"]);
    assert!(activity.contains("from the arg"), "activity: {activity}");
    assert!(
        !activity.contains("from the pipe"),
        "pipe content must be ignored when the positional is given: {activity}"
    );
}

#[test]
fn explicit_note_body_beats_piped_stdin() {
    let db = seeded("note_arg_wins");
    let r = run_piped_stdin(
        &db,
        &[
            "project",
            "note",
            "add",
            "SF",
            "T",
            "--body",
            "flag body",
        ],
        "pipe body\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let notes = ok_null(&db, &["project", "cat", "SF", "--section", "notes"]);
    assert!(notes.contains("flag body"), "notes: {notes}");
    assert!(!notes.contains("pipe body"), "notes: {notes}");
}

// --- empty pipe: validation error where text is required, no-op body where
// --- it is optional -----------------------------------------------------------

#[test]
fn empty_pipe_log_is_a_clean_validation_error() {
    let db = seeded("log_empty");
    let r = run_null_stdin(&db, &["issue", "log", "SF-1"]);
    assert_eq!(r.code, 2, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("message required"),
        "stderr: {}",
        r.stderr
    );
}

#[test]
fn empty_pipe_append_section_is_a_clean_validation_error() {
    let db = seeded("append_empty");
    let r = run_null_stdin(
        &db,
        &["issue", "append-section", "SF-1", "--section", "notes"],
    );
    assert_eq!(r.code, 2, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("nothing to append"),
        "stderr: {}",
        r.stderr
    );
}

#[test]
fn empty_pipe_note_add_keeps_the_bare_heading_note() {
    // Body is optional for project notes: an empty pipe means "no body",
    // exactly like today's scripted `note add` with stdin at /dev/null.
    let db = seeded("note_empty");
    let r = run_null_stdin(&db, &["project", "note", "add", "SF", "Bare"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let notes = ok_null(&db, &["project", "cat", "SF", "--section", "notes"]);
    assert!(notes.contains("### Bare"), "notes: {notes}");
}

// --- an explicit `-` on the value flag reads stdin ---------------------------
//
// The `*-file` flags have always honoured `-`; the value flags on `project`
// silently stored the dash itself, so a scripted `--body -` threw its body
// away and still exited 0.

// CLI-76
#[test]
fn dash_body_reads_stdin() {
    let db = seeded("body_dash");
    let r = run_piped_stdin(
        &db,
        &["project", "note", "add", "SF", "via body", "--body", "-"],
        "BODY_FROM_STDIN\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    // The only note on the board, so assert the whole section: a body of `-`
    // would otherwise still satisfy a `contains` check on the heading.
    let notes = ok_null(&db, &["project", "cat", "SF", "--section", "notes"]);
    assert_eq!(notes.trim(), "### via body\n\nBODY_FROM_STDIN", "{notes}");
}

// CLI-76
#[test]
fn dash_project_description_reads_stdin_on_add() {
    let db = tmp_db("proj_add_dash");
    let r = run_piped_stdin(
        &db,
        &["project", "add", "PD", "Dashed", "--description", "-"],
        "DESC_FROM_STDIN\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let shown = ok_null(&db, &["project", "show", "PD", "--json"]);
    assert!(shown.contains("DESC_FROM_STDIN"), "shown: {shown}");
}

// CLI-76
#[test]
fn dash_project_description_reads_stdin_on_edit() {
    let db = seeded("proj_edit_dash");
    let r = run_piped_stdin(
        &db,
        &["project", "edit", "SF", "--description", "-"],
        "EDITED_FROM_STDIN\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let shown = ok_null(&db, &["project", "show", "SF", "--json"]);
    assert!(shown.contains("EDITED_FROM_STDIN"), "shown: {shown}");
}

// CLI-76 — the sibling the spec asked to be checked for the same defect.
#[test]
fn dash_issue_description_reads_stdin() {
    let db = seeded("issue_desc_dash");
    let r = run_piped_stdin(
        &db,
        &[
            "issue",
            "add",
            "dashed",
            "--project",
            "SF",
            "--description",
            "-",
        ],
        "ISSUE_DESC_FROM_STDIN\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let shown = ok_null(&db, &["issue", "show", "SF-2", "--json"]);
    assert!(shown.contains("ISSUE_DESC_FROM_STDIN"), "shown: {shown}");
}

// CLI-76 — only a *bare* dash is the stdin sentinel; a markdown bullet that
// merely starts with one stays literal.
#[test]
fn hyphen_leading_body_is_not_the_stdin_sentinel() {
    let db = seeded("bullet_body");
    let r = run_piped_stdin(
        &db,
        &[
            "project",
            "note",
            "add",
            "SF",
            "Bulleted",
            "--body",
            "- a bullet",
        ],
        "FROM_THE_PIPE\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let notes = ok_null(&db, &["project", "cat", "SF", "--section", "notes"]);
    assert!(notes.contains("- a bullet"), "notes: {notes}");
    assert!(!notes.contains("FROM_THE_PIPE"), "notes: {notes}");
}

// CLI-76 — the shared resolver names the flags the command actually has;
// `note add` used to report the mutual exclusion as `--description`.
#[test]
fn mutually_exclusive_error_names_the_commands_own_flags() {
    let db = seeded("mutex_flags");
    let r = run_null_stdin(
        &db,
        &[
            "project",
            "note",
            "add",
            "SF",
            "T",
            "--body",
            "x",
            "--body-file",
            "y",
        ],
    );
    assert_eq!(r.code, 2, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(
        r.stderr
            .contains("--body and --body-file are mutually exclusive"),
        "stderr: {}",
        r.stderr
    );
}

// CLI-76 — the arm that always worked keeps working.
#[test]
fn dash_body_file_still_reads_stdin() {
    let db = seeded("body_file_dash");
    let r = run_piped_stdin(
        &db,
        &[
            "project",
            "note",
            "add",
            "SF",
            "via file",
            "--body-file",
            "-",
        ],
        "FILE_ARM_FROM_STDIN\n",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let notes = ok_null(&db, &["project", "cat", "SF", "--section", "notes"]);
    assert!(notes.contains("FILE_ARM_FROM_STDIN"), "notes: {notes}");
}

// --- a real TTY with no argument keeps the fast error, never blocks ----------

/// Open a pty pair and hand back (master fd, slave file). The master stays
/// open for the test's lifetime so the slave keeps a live peer — closing it
/// early would make the slave EOF, which is exactly the non-TTY behaviour we
/// are NOT testing.
#[cfg(unix)]
fn open_pty() -> (std::fs::File, std::fs::File) {
    use std::os::unix::io::FromRawFd;
    unsafe {
        let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        assert!(master >= 0, "posix_openpt failed");
        assert_eq!(libc::grantpt(master), 0, "grantpt failed");
        assert_eq!(libc::unlockpt(master), 0, "unlockpt failed");
        let mut buf = [0 as libc::c_char; 256];
        assert_eq!(
            libc::ptsname_r(master, buf.as_mut_ptr(), buf.len()),
            0,
            "ptsname_r failed"
        );
        let name = std::ffi::CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .to_string();
        let slave = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&name)
            .expect("open pty slave");
        (std::fs::File::from_raw_fd(master), slave)
    }
}

#[cfg(unix)]
#[test]
fn tty_stdin_with_no_argument_errors_fast_and_never_blocks() {
    use std::time::{Duration, Instant};

    let db = seeded("tty_fast");
    let (_master, slave) = open_pty();
    let mut child = base_cmd(&db, &["issue", "log", "SF-1"])
        .stdin(Stdio::from(slave))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cliban");

    // Nothing is ever written to the pty master. A correct build errors out
    // immediately; a regression that reads stdin on a TTY blocks here, so
    // poll with a deadline and fail loudly instead of hanging the suite.
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("`issue log` with a TTY stdin and no message blocked instead of erroring fast");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(status.code(), Some(2), "want the fast validation error");
    let out = child.wait_with_output().expect("collect output");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("message required"), "stderr: {stderr}");
}
