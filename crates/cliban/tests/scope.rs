//! `$CLIBAN_PROJECT` — the ambient scope, end to end.
//!
//! The contract: the env var is the default `-p` everywhere, an explicit
//! `-p KEY` beats it, `-p '*'` deliberately widens back out, and it never
//! fills positional identity.

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
        .join(format!("cliban_scope_{tag}_{nanos}.db"))
        .to_string_lossy()
        .to_string()
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

fn run_scoped(db: &str, scope: Option<&str>, args: &[&str]) -> Run {
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
    if let Some(s) = scope {
        cmd.env("CLIBAN_PROJECT", s);
    }
    let out = cmd.output().expect("run cliban");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn ok_scoped(db: &str, scope: Option<&str>, args: &[&str]) -> String {
    let r = run_scoped(db, scope, args);
    assert_eq!(
        r.code,
        0,
        "`cliban {}` (scope {scope:?}) failed: {}",
        args.join(" "),
        r.stderr
    );
    r.stdout
}

fn seeded(tag: &str) -> String {
    let db = tmp_db(tag);
    ok_scoped(&db, None, &["project", "add", "AA", "--name", "A"]);
    ok_scoped(&db, None, &["project", "add", "BB", "--name", "B"]);
    ok_scoped(&db, None, &["issue", "add", "in a", "--project", "AA"]);
    ok_scoped(&db, None, &["issue", "add", "in b", "--project", "BB"]);
    db
}

#[test]
fn env_scope_is_the_default_project_and_is_upcased() {
    let db = seeded("default");
    // Writes land in the scoped project without -p (lowercase env upcased).
    let echo = ok_scoped(&db, Some("aa"), &["issue", "add", "scoped", "--json"]);
    assert!(echo.contains(r#""key":"AA-2""#), "got {echo}");
    // Reads are scoped the same way.
    let rows = ok_scoped(&db, Some("AA"), &["issue", "ls", "--json"]);
    assert!(rows.lines().all(|l| l.contains(r#""key":"AA-"#)), "{rows}");
}

#[test]
fn explicit_flag_beats_the_env_and_star_widens() {
    let db = seeded("beats");
    let rows = ok_scoped(&db, Some("AA"), &["issue", "ls", "-p", "BB", "--json"]);
    assert!(rows.lines().all(|l| l.contains(r#""key":"BB-"#)), "{rows}");
    let all = ok_scoped(&db, Some("AA"), &["issue", "ls", "-p", "*", "--json"]);
    assert!(
        all.contains(r#""key":"AA-1""#) && all.contains(r#""key":"BB-1""#),
        "-p '*' must span every project: {all}"
    );
    // milestone ls under a scope shows rows; -p '*' falls back to the summary.
    ok_scoped(&db, None, &["milestone", "add", "v1", "--project", "AA"]);
    let scoped = ok_scoped(&db, Some("AA"), &["milestone", "ls", "--json"]);
    assert!(scoped.contains(r#""name":"v1""#), "{scoped}");
    let summary = ok_scoped(&db, Some("AA"), &["milestone", "ls", "-p", "*", "--json"]);
    assert!(
        summary.contains(r#""milestones":1"#) && !summary.contains(r#""name""#),
        "{summary}"
    );
}

#[test]
fn missing_scope_errors_name_both_spellings() {
    let db = seeded("missing");
    let r = run_scoped(&db, None, &["issue", "add", "nowhere"]);
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert!(
        r.stderr.contains("-p KEY") && r.stderr.contains("CLIBAN_PROJECT"),
        "the error must teach both spellings: {}",
        r.stderr
    );
}

#[test]
fn env_never_fills_positional_identity() {
    let db = seeded("identity");
    // `project cat` addresses a project BY KEY positionally; the scope must
    // not stand in for it.
    let r = run_scoped(&db, Some("AA"), &["project", "cat"]);
    assert_ne!(r.code, 0, "positional identity must stay required");
    assert!(r.stderr.contains("required"), "{}", r.stderr);
    let _ = r.stdout;
}
