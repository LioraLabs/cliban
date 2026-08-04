//! Leading-hyphen payloads through the real binary. Markdown bullets and
//! flag-like titles are ordinary text-flag values; clap must not eat them
//! (three separate commands died on this in one dogfooding session).

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
        .join(format!("cliban_hyphen_{tag}_{nanos}.db"))
        .to_string_lossy()
        .to_string()
}

fn ok(db: &str, args: &[&str]) -> String {
    let out = Command::new(bin())
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
    assert!(
        out.status.success(),
        "`cliban {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn hyphen_leading_values_survive_space_separated_form() {
    let db = tmp_db("main");
    ok(&db, &["project", "add", "HY", "Hyphens"]);

    // A flag-looking TITLE needs the standard `--` escape (a positional that
    // swallowed hyphens would also swallow typo'd flags); hyphen-leading
    // --description values still pass bare.
    ok(
        &db,
        &[
            "issue",
            "add",
            "--project",
            "HY",
            "--description",
            "- a markdown bullet",
            "--",
            "--flag-looking title",
        ],
    );
    let shown = ok(&db, &["issue", "show", "HY-1", "--json"]);
    assert!(shown.contains("--flag-looking title"), "{shown}");
    assert!(shown.contains("- a markdown bullet"), "{shown}");

    // mv --note with a bullet.
    ok(
        &db,
        &[
            "issue",
            "mv",
            "HY-1",
            "in-progress",
            "--note",
            "- because reasons",
        ],
    );

    // log message positional with a bullet.
    ok(&db, &["issue", "log", "HY-1", "- found a thing"]);

    // project note add with a flag-looking title (via the `--` escape, same
    // as any flag-looking positional) and a bullet body.
    ok(
        &db,
        &[
            "project",
            "note",
            "add",
            "--body",
            "- the lesson",
            "--",
            "HY",
            "--section files carry the body only",
        ],
    );
    let notes = ok(&db, &["project", "cat", "HY", "--section", "notes"]);
    assert!(
        notes.contains("--section files carry the body only"),
        "{notes}"
    );
}
