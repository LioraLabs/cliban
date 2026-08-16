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

// The partition is the whole output: open waves, done, externally gated. The
// `related_to` edges here are deliberate — they used to group tickets onto one
// agent, and this fixture is what catches that coming back.
#[test]
fn waves_partition_open_work_and_ignore_related_to() {
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
    let positions = ["collisions", "done", "external_blocked", "waves"]
        .map(|key| raw.find(&format!("\"{key}\"")).unwrap());
    assert!(positions.is_sorted(), "{raw}");
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(json["chains"], serde_json::Value::Null, "{raw}");
    assert_eq!(
        json["waves"],
        serde_json::json!([["CLI-1", "CLI-2", "CLI-3", "CLI-4"]])
    );
    assert_eq!(json["done"], serde_json::json!(["CLI-5"]));
    assert_eq!(json["external_blocked"], serde_json::json!([]));

    let table = run(db, &["milestone", "waves", "M", "-p", "CLI", "--table"]);
    assert!(!table.contains("chains"), "{table}");
}

fn files(paths: &[&str]) -> String {
    let mut s = String::from("## Spec\n\nwhat it does\n\n## Files\n\n");
    for p in paths {
        s.push_str(&format!("- M {p}\n"));
    }
    s
}

// Two tickets that would run at the same time and predict touching one path
// are reported as a collision and still dispatched in parallel. The prediction
// is file-granular — it cannot tell two line-adds from two incompatible designs
// — so it briefs the agents rather than scheduling them.
#[test]
fn waves_report_a_same_wave_path_collision_without_chaining() {
    let db = std::env::temp_dir().join(format!("cliban_waves_collide_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    let a = files(&["src/shared.rs", "src/only_a.rs"]);
    let b = files(&["src/shared.rs"]);
    let c = files(&["src/only_c.rs"]);
    run(db, &["issue", "add", "a", "-p", "CLI", "-m", "M", "--description", &a]);
    run(db, &["issue", "add", "b", "-p", "CLI", "-m", "M", "--description", &b]);
    run(db, &["issue", "add", "c", "-p", "CLI", "-m", "M", "--description", &c]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["waves"],
        serde_json::json!([["CLI-1", "CLI-2", "CLI-3"]]),
        "{raw}"
    );
    assert_eq!(
        json["collisions"],
        serde_json::json!([{"keys": ["CLI-1", "CLI-2"], "path": "src/shared.rs"}]),
        "{raw}"
    );

    let table = run(db, &["milestone", "waves", "M", "-p", "CLI", "--table"]);
    assert!(
        table.contains("collision: src/shared.rs predicted by CLI-1, CLI-2"),
        "{table}"
    );
}

// The discriminator: tickets in DIFFERENT waves are already serialised by the
// dependency graph, so a shared path is not a collision. An implementation that
// intersected predictions across the whole milestone rather than within one wave
// would report these two, and this fixture is what catches it.
#[test]
fn waves_do_not_collide_tickets_the_graph_already_serialises() {
    let db = std::env::temp_dir().join(format!("cliban_waves_nocollide_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    let shared = files(&["src/shared.rs"]);
    run(db, &["issue", "add", "first", "-p", "CLI", "-m", "M", "--description", &shared]);
    run(db, &["issue", "add", "second", "-p", "CLI", "-m", "M", "--description", &shared]);
    run(db, &["issue", "edit", "CLI-2", "--blocked-by", "CLI-1"]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(json["collisions"], serde_json::json!([]), "{raw}");
    assert_eq!(
        json["waves"],
        serde_json::json!([["CLI-1"], ["CLI-2"]]),
        "{raw}"
    );
}

// A collision is a briefing input, never a schedule: the colliding sibling
// still runs in its own wave alongside the spine's head, and the overlap is
// reported so the orchestrator can name it in both briefs.
#[test]
fn a_collision_is_reported_without_reordering_the_waves() {
    let db = std::env::temp_dir().join(format!("cliban_waves_frag_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    let shared = files(&["src/shared.rs"]);
    run(db, &["issue", "add", "head", "-p", "CLI", "-m", "M", "--description", &shared]);
    for title in ["mid", "tail"] {
        run(db, &["issue", "add", title, "-p", "CLI", "-m", "M"]);
    }
    run(db, &["issue", "add", "sibling", "-p", "CLI", "-m", "M", "--description", &shared]);
    run(db, &["issue", "edit", "CLI-2", "--blocked-by", "CLI-1"]);
    run(db, &["issue", "edit", "CLI-3", "--blocked-by", "CLI-2"]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["waves"],
        serde_json::json!([["CLI-1", "CLI-4"], ["CLI-2"], ["CLI-3"]]),
        "{raw}"
    );
    assert_eq!(
        json["collisions"],
        serde_json::json!([{"keys": ["CLI-1", "CLI-4"], "path": "src/shared.rs"}]),
        "{raw}"
    );
}

// Blocking edges are the only thing that layers the work: a linear run is one
// ticket per wave until it forks, and the fork is where it genuinely splits.
#[test]
fn waves_layer_a_linear_run_and_split_at_the_fork() {
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
        json["waves"],
        serde_json::json!([["CLI-1"], ["CLI-2"], ["CLI-3"], ["CLI-4", "CLI-5"]]),
        "{raw}"
    );
}

// Work this milestone can never start is named as gated, not scheduled — and
// the gate is transitive down the run.
#[test]
fn waves_gate_issues_blocked_from_outside_the_milestone() {
    let db = std::env::temp_dir().join(format!("cliban_waves_gated_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    for title in ["head", "gated", "tail"] {
        run(db, &["issue", "add", title, "-p", "CLI", "-m", "M"]);
    }
    run(db, &["issue", "add", "outsider", "-p", "CLI"]);
    // CLI-2 is blocked from inside the milestone AND from outside it, so the
    // run CLI-1 -> CLI-2 -> CLI-3 is unschedulable from CLI-2 onward.
    run(db, &["issue", "edit", "CLI-2", "--blocked-by", "CLI-1"]);
    run(db, &["issue", "edit", "CLI-3", "--blocked-by", "CLI-2"]);
    run(db, &["issue", "edit", "CLI-2", "--blocked-by", "CLI-4"]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["external_blocked"],
        serde_json::json!(["CLI-2", "CLI-3"]),
        "{raw}"
    );
}

// The gated issue is two hops from the outside blocker, so it never enters the
// raw single-hop `external` set and only the fixpoint excludes it.
#[test]
fn waves_gate_an_issue_two_hops_from_the_outside_blocker() {
    let db = std::env::temp_dir().join(format!("cliban_waves_reltrans_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    for title in ["a", "gated", "gate"] {
        run(db, &["issue", "add", title, "-p", "CLI", "-m", "M"]);
    }
    run(db, &["issue", "add", "outsider", "-p", "CLI"]);
    run(db, &["issue", "edit", "CLI-1", "--related-to", "CLI-2"]);
    run(db, &["issue", "edit", "CLI-2", "--blocked-by", "CLI-3"]);
    run(db, &["issue", "edit", "CLI-3", "--blocked-by", "CLI-4"]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["external_blocked"],
        serde_json::json!(["CLI-2", "CLI-3"]),
        "{raw}"
    );
    assert_eq!(json["waves"], serde_json::json!([["CLI-1"]]), "{raw}");
}

// The gate sits on the HEAD of a three-node run, so the tail is only
// transitively unschedulable and only a real fixpoint over `external_blocked`
// catches it.
#[test]
fn waves_gate_the_whole_tail_below_a_gated_head() {
    let db = std::env::temp_dir().join(format!("cliban_waves_transext_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    for title in ["head", "mid", "tail"] {
        run(db, &["issue", "add", title, "-p", "CLI", "-m", "M"]);
    }
    run(db, &["issue", "add", "outsider", "-p", "CLI"]);
    run(db, &["issue", "edit", "CLI-2", "--blocked-by", "CLI-1"]);
    run(db, &["issue", "edit", "CLI-3", "--blocked-by", "CLI-2"]);
    run(db, &["issue", "edit", "CLI-1", "--blocked-by", "CLI-4"]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["external_blocked"],
        serde_json::json!(["CLI-1", "CLI-2", "CLI-3"]),
        "{raw}"
    );
}

// A `related_to` edge across a blocking run changes nothing: the run layers by
// its blocking edges and the related ticket schedules in the first wave with
// everything else that is ready.
#[test]
fn waves_layer_a_run_unaffected_by_a_related_to_edge() {
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
        json["waves"],
        serde_json::json!([["CLI-1", "CLI-5"], ["CLI-2"], ["CLI-3"], ["CLI-4"]]),
        "{raw}"
    );
}
