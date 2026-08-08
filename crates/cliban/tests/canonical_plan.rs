//! A `## Plan` is stored canonically: whatever an agent hands in, what lands
//! in the DB is something `tick` can drive.
//!
//! A flat plan — checkbox steps with no `### Task N:` heading — used to be
//! accepted verbatim at write time, reported by `lint` afterwards, and refused
//! by `tick` ("no `### Task N:` headings in ## Plan"). The contract is not
//! relaxed to admit flat plans; they are made canonical on the way in.

use std::process::Command;

fn run(db: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cliban"))
        .arg("--db")
        .arg(db)
        .env_remove("CLIBAN_OUTPUT")
        .env_remove("CLIBAN_PROJECT")
        .args(args)
        .output()
        .expect("run cliban")
}

fn out(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn db_at(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "cliban_canonical_plan_{tag}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db = root.join("board.db");
    assert!(run(&db, &["project", "add", "CP", "Canonical"])
        .status
        .success());
    db
}

fn add(db: &std::path::Path, title: &str, desc: &str) -> String {
    let o = run(
        db,
        &[
            "issue",
            "add",
            title,
            "-p",
            "CP",
            "--description",
            desc,
            "--json",
        ],
    );
    assert!(
        o.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_str::<serde_json::Value>(&out(&o)).unwrap()["key"]
        .as_str()
        .unwrap()
        .to_string()
}

fn plan_of(db: &std::path::Path, key: &str) -> String {
    out(&run(db, &["issue", "cat", key, "--section", "plan"]))
}

#[test]
fn a_flat_plan_is_stored_with_its_implicit_task_one() {
    let db = db_at("flat");
    let key = add(
        &db,
        "Rewire the parser",
        "## Spec\n\ns\n\n## Plan\n\n- [ ] first\n- [ ] second\n",
    );

    let plan = plan_of(&db, &key);
    assert!(
        plan.contains("### Task 1: Rewire the parser"),
        "flat plan gained its task heading: {plan:?}"
    );
    assert!(plan.contains("- [ ] first"), "steps survive: {plan:?}");
    assert!(plan.contains("- [ ] second"));

    // The stored shape lints clean — the contract was not relaxed to admit
    // the flat plan, the flat plan was made to satisfy it.
    let lint = run(&db, &["issue", "lint", &key, "--table"]);
    assert!(lint.status.success(), "lint: {}", out(&lint));
    assert!(!out(&lint).contains("tick cannot reach"), "{}", out(&lint));

    // …and `tick` can now address the steps it previously refused.
    let tick = run(&db, &["issue", "tick", &key, "--task", "1", "--step", "1"]);
    assert!(
        tick.status.success(),
        "tick: {}",
        String::from_utf8_lossy(&tick.stderr)
    );
    assert!(plan_of(&db, &key).contains("- [x] first"));
}

#[test]
fn a_plan_that_already_has_task_headings_is_stored_untouched() {
    let db = db_at("shaped");
    let body = "## Spec\n\ns\n\n## Plan\n\n### Task 1: a\n\n- [ ] x\n\n### Task 2: b\n\n- [ ] y\n";
    let key = add(&db, "Already canonical", body);

    let plan = plan_of(&db, &key);
    assert!(plan.contains("### Task 1: a"), "{plan:?}");
    assert!(plan.contains("### Task 2: b"), "{plan:?}");
    assert!(
        !plan.contains("Already canonical"),
        "no task heading was invented: {plan:?}"
    );
}

#[test]
fn an_issue_with_no_plan_is_stored_untouched() {
    let db = db_at("noplan");
    let key = add(&db, "Spec only", "## Spec\n\njust a spec, no plan at all\n");
    let o = run(&db, &["issue", "cat", &key, "--section", "plan"]);
    assert!(!o.status.success(), "there is still no plan to read");
    assert!(!out(&run(&db, &["issue", "show", &key])).contains("Task 1"));
}

#[test]
fn a_plan_with_no_steps_does_not_gain_an_empty_task() {
    // The Linear bridge seeds a `## Plan` placeholder. An empty task heading
    // would trade one lint finding for another ("Task 1 has no steps").
    let db = db_at("nosteps");
    let key = add(
        &db,
        "Placeholder",
        "## Spec\n\ns\n\n## Plan\n\n_No plan yet._\n",
    );
    let plan = plan_of(&db, &key);
    assert!(!plan.contains("### Task 1"), "no invented task: {plan:?}");
}

#[test]
fn editing_a_description_canonicalizes_it_too() {
    let db = db_at("edit");
    let key = add(&db, "Grows a plan", "## Spec\n\ns\n");
    let o = run(
        &db,
        &[
            "issue",
            "edit",
            &key,
            "--description",
            "## Spec\n\ns\n\n## Plan\n\n- [ ] only step\n",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(plan_of(&db, &key).contains("### Task 1: Grows a plan"));
}

#[test]
fn tick_still_infers_the_task_when_the_plan_has_exactly_one() {
    // Canonicalization does not make this inference redundant — it makes it
    // load-bearing. Every flat plan an agent writes now becomes an
    // exactly-one-task plan, so this is the path those issues tick through.
    let db = db_at("infer");
    let key = add(
        &db,
        "Single task",
        "## Spec\n\ns\n\n## Plan\n\n- [ ] only step\n",
    );
    let o = run(&db, &["issue", "tick", &key, "--step", "1", "--json"]);
    assert!(
        o.status.success(),
        "--task must stay optional: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(&out(&o)).unwrap();
    assert_eq!(v["task"], 1);
    assert!(plan_of(&db, &key).contains("- [x] only step"));
}
