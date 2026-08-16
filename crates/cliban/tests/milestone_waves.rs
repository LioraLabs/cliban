use std::process::Command;

fn run(db: &str, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_cliban"))
        .arg("--db")
        .arg(db)
        .args(args)
        .env_remove("CLIBAN_PROJECT")
        .output()
        .expect("run cliban");
    assert!(
        out.status.success(),
        "cliban {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn waves_add_advisory_open_milestone_chains() {
    let db = std::env::temp_dir().join(format!("cliban_waves_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    for title in ["a", "b", "c", "unrelated", "done", "archived"] {
        run(db, &["issue", "add", title, "-p", "CLI", "-m", "M"]);
    }
    run(db, &["issue", "add", "external", "-p", "CLI"]);
    run(db, &["issue", "edit", "CLI-1", "--related-to", "CLI-2"]);
    run(db, &["issue", "edit", "CLI-2", "--related-to", "CLI-3"]);
    run(db, &["issue", "edit", "CLI-1", "--related-to", "CLI-5"]);
    run(db, &["issue", "edit", "CLI-2", "--related-to", "CLI-6"]);
    run(db, &["issue", "edit", "CLI-3", "--related-to", "CLI-7"]);
    run(db, &["issue", "mv", "CLI-5", "done"]);
    run(db, &["issue", "archive", "CLI-6"]);

    let raw = run(
        db,
        &["milestone", "waves", "M", "-p", "CLI", "--json"],
    );
    let positions = ["chains", "done", "external_blocked", "waves"]
        .map(|key| raw.find(&format!("\"{key}\"")).unwrap());
    assert!(positions.is_sorted(), "{raw}");
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["chains"],
        serde_json::json!([["CLI-1", "CLI-2", "CLI-3"]])
    );
    assert_eq!(
        json["waves"],
        serde_json::json!([["CLI-1", "CLI-2", "CLI-3", "CLI-4"]])
    );
    assert_eq!(json["done"], serde_json::json!(["CLI-5"]));
    assert_eq!(json["external_blocked"], serde_json::json!([]));

    let table = run(db, &["milestone", "waves", "M", "-p", "CLI", "--table"]);
    assert!(table.contains("chains: [CLI-1, CLI-2, CLI-3]"), "{table}");
}

// A serialised spine is where one implementer saves the most: same surface,
// one wave after another. Fan-out is where the work genuinely splits.
#[test]
fn waves_chain_a_linear_blocking_run_until_it_forks() {
    let db = std::env::temp_dir().join(format!("cliban_waves_spine_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    for title in ["head", "mid", "fork", "left", "right"] {
        run(db, &["issue", "add", title, "-p", "CLI", "-m", "M"]);
    }
    run(db, &["issue", "edit", "CLI-2", "--blocked-by", "CLI-1"]);
    run(db, &["issue", "edit", "CLI-3", "--blocked-by", "CLI-2"]);
    run(db, &["issue", "edit", "CLI-4", "--blocked-by", "CLI-3"]);
    run(db, &["issue", "edit", "CLI-5", "--blocked-by", "CLI-3"]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["chains"],
        serde_json::json!([["CLI-1", "CLI-2", "CLI-3"]]),
        "{raw}"
    );
    assert_eq!(
        json["waves"],
        serde_json::json!([["CLI-1"], ["CLI-2"], ["CLI-3"], ["CLI-4", "CLI-5"]]),
        "{raw}"
    );
}

// An author-approved `related_to` group outranks an inferred run, so the run
// splits around it instead of claiming its members twice.
#[test]
fn waves_split_a_run_around_an_explicit_related_group() {
    let db = std::env::temp_dir().join(format!("cliban_waves_split_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    for title in ["one", "two", "three", "four", "affine"] {
        run(db, &["issue", "add", title, "-p", "CLI", "-m", "M"]);
    }
    run(db, &["issue", "edit", "CLI-2", "--blocked-by", "CLI-1"]);
    run(db, &["issue", "edit", "CLI-3", "--blocked-by", "CLI-2"]);
    run(db, &["issue", "edit", "CLI-4", "--blocked-by", "CLI-3"]);
    run(db, &["issue", "edit", "CLI-2", "--related-to", "CLI-5"]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["chains"],
        serde_json::json!([["CLI-2", "CLI-5"], ["CLI-3", "CLI-4"]]),
        "{raw}"
    );
}
