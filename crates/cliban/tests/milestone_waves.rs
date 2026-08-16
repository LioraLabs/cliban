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

fn files(paths: &[&str]) -> String {
    let mut s = String::from("## Spec\n\nwhat it does\n\n## Files\n\n");
    for p in paths {
        s.push_str(&format!("- M {p}\n"));
    }
    s
}

// Two tickets that would run at the same time and predict touching one path
// are joined, so one implementer takes them in sequence.
#[test]
fn waves_chain_same_wave_tickets_that_predict_one_path() {
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
        json["chains"],
        serde_json::json!([["CLI-1", "CLI-2"]]),
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
// dependency graph, so a shared path is not a collision. An implementation
// that intersected predictions across the whole milestone rather than within
// one wave would chain these two, and this fixture is what catches it.
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
    // Still chained, but by the blocking run and in dependency order.
    assert_eq!(
        json["chains"],
        serde_json::json!([["CLI-1", "CLI-2"]]),
        "{raw}"
    );
}

// A collision claims its members, so a run passing through one splits around
// it, exactly as it does around an author-approved `related_to` group. The
// head is staffed with the ticket it clashes with; the rest of the run stays a
// run. Nobody appears in two chains.
#[test]
fn a_collision_splits_a_run_the_way_a_related_group_does() {
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
        json["chains"],
        serde_json::json!([["CLI-1", "CLI-4"], ["CLI-2", "CLI-3"]]),
        "{raw}"
    );
    assert_eq!(
        json["collisions"],
        serde_json::json!([{"keys": ["CLI-1", "CLI-4"], "path": "src/shared.rs"}]),
        "{raw}"
    );
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

// A chain is advice about who works what next, so it must not name a ticket
// this milestone can never start.
#[test]
fn waves_keep_externally_gated_issues_out_of_chains() {
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
    assert_eq!(json["chains"], serde_json::json!([]), "{raw}");
    assert_eq!(
        json["external_blocked"],
        serde_json::json!(["CLI-2", "CLI-3"]),
        "{raw}"
    );
}

// Reversed key order: a run is emitted in dependency order, so sorting it
// would be a regression this fixture catches and an ascending one would not.
#[test]
fn waves_emit_a_run_in_dependency_order_not_key_order() {
    let db = std::env::temp_dir().join(format!("cliban_waves_order_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    for title in ["last", "first"] {
        run(db, &["issue", "add", title, "-p", "CLI", "-m", "M"]);
    }
    run(db, &["issue", "edit", "CLI-1", "--blocked-by", "CLI-2"]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["chains"],
        serde_json::json!([["CLI-2", "CLI-1"]]),
        "{raw}"
    );
}

// `related_to` groups are gated by the same `schedulable` predicate as
// inferred runs: a member that is externally blocked drops out of the group,
// not just out of blocking-run chains. Here the group shrinks to one member
// and a one-element group is not a chain.
#[test]
fn waves_drop_externally_blocked_members_from_related_groups() {
    let db = std::env::temp_dir().join(format!("cliban_waves_relext_{}.db", std::process::id()));
    let db = db.to_str().unwrap();
    run(db, &["project", "add", "CLI", "Cliban"]);
    run(db, &["milestone", "add", "M", "-p", "CLI"]);
    for title in ["a", "b"] {
        run(db, &["issue", "add", title, "-p", "CLI", "-m", "M"]);
    }
    run(db, &["issue", "add", "outsider", "-p", "CLI"]);
    run(db, &["issue", "edit", "CLI-1", "--related-to", "CLI-2"]);
    run(db, &["issue", "edit", "CLI-1", "--blocked-by", "CLI-3"]);

    let raw = run(db, &["milestone", "waves", "M", "-p", "CLI", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(json["chains"], serde_json::json!([]), "{raw}");
    assert_eq!(
        json["external_blocked"],
        serde_json::json!(["CLI-1"]),
        "{raw}"
    );
}

// The `related_to` mirror of the transitive case: the gated member is two hops
// from the outside blocker, so it never enters the raw single-hop `external`
// set and only the fixpoint excludes it. A filter reading `external` instead of
// `external_blocked` keeps the pairing and every other test still passes.
#[test]
fn waves_drop_transitively_gated_members_from_related_groups() {
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
    assert_eq!(json["chains"], serde_json::json!([]), "{raw}");
    assert_eq!(
        json["external_blocked"],
        serde_json::json!(["CLI-2", "CLI-3"]),
        "{raw}"
    );
}

// The existing external-gating test puts the gate on a node adjacent to both
// excluded neighbors, which a shallow "check direct external membership on
// both sides" fix (no fixpoint) would also pass. This one puts the gate on
// the HEAD of a three-node run, so the tail is only transitively
// unschedulable and only a real fixpoint over `external_blocked` catches it.
#[test]
fn waves_keep_transitively_gated_tail_out_of_chains() {
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
    assert_eq!(json["chains"], serde_json::json!([]), "{raw}");
    assert_eq!(
        json["external_blocked"],
        serde_json::json!(["CLI-1", "CLI-2", "CLI-3"]),
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
