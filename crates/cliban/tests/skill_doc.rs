//! Keeps `plugin/skills/cliban/SKILL.md` honest.
//!
//! The skill is the map agents navigate by, so a command that gets renamed or
//! dropped without the doc following turns into an agent fumbling through
//! `--help`. This walks every `cliban ...` invocation in the skill's shell
//! blocks and asserts the subcommand still exists.
//!
//! Flags are deliberately not checked here — that would duplicate clap's
//! definitions. The command tree is what actually moves.

use std::collections::BTreeSet;
use std::process::Command;

/// SKILL.md plus every reference file it progressively discloses — a command
/// documented only in `references/` must not drift either.
fn skill_md() -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/skills/cliban");
    let mut md = std::fs::read_to_string(dir.join("SKILL.md"))
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.join("SKILL.md").display()));
    if let Ok(refs) = std::fs::read_dir(dir.join("references")) {
        for entry in refs.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                md.push('\n');
                md.push_str(
                    &std::fs::read_to_string(&path)
                        .unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
                );
            }
        }
    }
    md
}

/// Subcommand paths named in the doc. cliban's tree is at most two deep
/// (`group subcommand`), so take up to two leading non-flag words.
fn documented_paths(md: &str) -> BTreeSet<Vec<String>> {
    let mut out = BTreeSet::new();
    let mut in_code = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        // Only shell blocks — prose can legitimately start a sentence with
        // "cliban records ...".
        if !in_code {
            continue;
        }
        let line = line.trim();
        let Some(rest) = line.strip_prefix("cliban ") else {
            continue;
        };
        let path: Vec<String> = rest
            // Stop at the first flag, trailing `# comment`, or shell operator.
            .split_whitespace()
            .take_while(|w| !w.starts_with(['-', '#', '<', '>', '|', '$']))
            .take(2)
            .map(str::to_string)
            .collect();
        if !path.is_empty() {
            out.insert(path);
        }
    }
    out
}

/// Commands that only exist when an optional feature is compiled in. The doc
/// describes the default build, so under `--no-default-features` these are
/// legitimately absent and must not be reported as drift.
fn feature_gated(path: &[String]) -> bool {
    let gated = cfg!(not(feature = "linear")) && matches!(path[0].as_str(), "import" | "push");
    // `issue import` is unrelated to the Linear bridge and always present.
    gated && path.len() > 1
}

#[test]
fn every_command_the_skill_names_exists() {
    let md = skill_md();
    let paths = documented_paths(&md);
    assert!(
        paths.len() > 20,
        "expected the skill to document a real command surface, found {}",
        paths.len()
    );

    let mut missing = Vec::new();
    for path in paths.iter().filter(|p| !feature_gated(p)) {
        let out = Command::new(env!("CARGO_BIN_EXE_cliban"))
            .args(path)
            .arg("--help")
            .output()
            .expect("run cliban --help");
        if !out.status.success() {
            missing.push(path.join(" "));
        }
    }
    assert!(
        missing.is_empty(),
        "SKILL.md documents commands that no longer exist: {missing:?}"
    );
}

#[test]
fn the_skill_names_every_top_level_command() {
    // The reverse direction: a command the skill never mentions is a command
    // agents will not know about.
    let md = skill_md();
    let out = Command::new(env!("CARGO_BIN_EXE_cliban"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    let commands: Vec<&str> = help
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .filter(|c| !matches!(*c, "help"))
        .collect();
    assert!(!commands.is_empty(), "could not parse `cliban --help`");

    let undocumented: Vec<&str> = commands
        .iter()
        .copied()
        .filter(|c| !md.contains(&format!("`{c}")) && !md.contains(&format!("cliban {c}")))
        .collect();
    assert!(
        undocumented.is_empty(),
        "these commands exist but SKILL.md never mentions them: {undocumented:?}"
    );
}
