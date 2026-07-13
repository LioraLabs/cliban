use std::io::Write;
use std::process::{Command, Stdio};

fn run(db: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cliban"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("run cliban")
}

fn run_stdin(db: &std::path::Path, args: &[&str], stdin: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cliban"))
        .arg("--db")
        .arg(db)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run cliban");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn project_notes_support_targeted_reads_and_progressive_search() {
    let root = std::env::temp_dir().join(format!("cliban_project_memory_{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let db = root.join("memory.db");
    let description = root.join("project.md");
    std::fs::write(
        &description,
        "## Spec\n\n### Design\n\nSQLite is not mentioned here.\n\n## Notes\n\nLegacy loose memory must not be returned wholesale.\n\n### Database\n\nSQLite is the canonical durable store.\n\n```markdown\n```not-a-close\n## Embedded\n### Fake\nfenced phantom\n```\n\n### UI\n\nÉcole labels require Unicode matching.\n",
    )
    .unwrap();

    assert!(run(
        &db,
        &[
            "project",
            "add",
            "MEM",
            "--name",
            "Memory",
            "--description-file",
            description.to_str().unwrap(),
        ],
    )
    .status
    .success());

    let notes = run(&db, &["project", "show", "MEM", "--section", "notes"]);
    let notes_stdout = String::from_utf8_lossy(&notes.stdout);
    assert!(notes.status.success());
    assert!(notes_stdout.contains("### Database"));
    assert!(!notes_stdout.contains("## Spec"));

    let database = run(
        &db,
        &[
            "project",
            "search",
            "MEM",
            "sqlte canon",
            "--section",
            "notes",
            "--json",
        ],
    );
    let database_stdout = String::from_utf8_lossy(&database.stdout);
    assert!(
        database.status.success(),
        "{}",
        String::from_utf8_lossy(&database.stderr)
    );
    assert_eq!(database_stdout.lines().count(), 1);
    assert!(database_stdout.contains("Database"));
    assert!(!database_stdout.contains("UI"));

    let fenced = run(
        &db,
        &[
            "project",
            "search",
            "MEM",
            "phantom",
            "--section",
            "notes",
            "--json",
        ],
    );
    let fenced_stdout = String::from_utf8_lossy(&fenced.stdout);
    assert!(fenced.status.success());
    assert!(
        fenced_stdout.contains(r#""heading":"Database""#),
        "{fenced_stdout}"
    );
    assert!(!fenced_stdout.contains(r#""heading":"Fake""#));

    let unicode = run(
        &db,
        &[
            "project",
            "search",
            "MEM",
            "école unicode",
            "--section",
            "notes",
            "--json",
        ],
    );
    assert!(unicode.status.success());
    assert!(String::from_utf8_lossy(&unicode.stdout).contains("UI"));

    let unsectioned = run(&db, &["project", "search", "MEM", "legacy loose", "--json"]);
    assert!(unsectioned.status.success());
    assert!(unsectioned.stdout.is_empty());
    let cross_h2 = run(
        &db,
        &[
            "project",
            "search",
            "MEM",
            "legacy loose",
            "--section",
            "all",
            "--json",
        ],
    );
    assert!(cross_h2.status.success());
    assert!(cross_h2.stdout.is_empty());

    assert!(!run(
        &db,
        &["project", "search", "MEM", "   ", "--section", "notes"]
    )
    .status
    .success());
    assert!(!run(
        &db,
        &[
            "project",
            "search",
            "MEM",
            "sqlite",
            "--section",
            "notes",
            "--limit",
            "0"
        ]
    )
    .status
    .success());

    let edited = run_stdin(
        &db,
        &["project", "edit", "MEM", "--description-file", "-"],
        "## Notes\n\n### Replacement\n\nUpdated from stdin.\n",
    );
    assert!(
        edited.status.success(),
        "{}",
        String::from_utf8_lossy(&edited.stderr)
    );
    let replacement = run(
        &db,
        &["project", "search", "MEM", "updated stdin", "--json"],
    );
    assert!(replacement.status.success());
    assert!(String::from_utf8_lossy(&replacement.stdout).contains("Replacement"));

    assert!(run(
        &db,
        &[
            "project",
            "add",
            "LIT",
            "--name",
            "Literal",
            "--description",
            "-",
        ],
    )
    .status
    .success());
    let literal = run(&db, &["project", "show", "LIT", "--json"]);
    assert!(String::from_utf8_lossy(&literal.stdout).contains(r#""description": "-""#));

    let _ = std::fs::remove_dir_all(root);
}
