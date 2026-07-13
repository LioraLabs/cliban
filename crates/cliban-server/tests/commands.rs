//! Unit tests for the control-command router — no SSH involved.

use cliban_server::commands::{self, USAGE};
use cliban_server::config::{ServerConfig, SignupPolicy};
use cliban_server::server::{AppState, KeyInfo};

fn temp_data_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cliband-cmd-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn state(policy: SignupPolicy, token: Option<&str>) -> AppState {
    let cfg = ServerConfig {
        data_dir: temp_data_dir(),
        signup_policy: policy,
        signup_token: token.map(String::from),
        ..ServerConfig::default()
    };
    AppState::from_config(&cfg).unwrap()
}

fn key(name: &str) -> KeyInfo {
    KeyInfo {
        fingerprint: format!("SHA256:fake-{name}"),
        openssh: format!("ssh-ed25519 AAAAfake {name}"),
        username: name.to_string(),
        user: None,
    }
}

#[test]
fn signup_open_enrolls_key_as_owner() {
    let st = state(SignupPolicy::Open, None);
    let mut alice = key("alice");

    let out = commands::run(&st, &mut alice, "signup team-a");
    assert_eq!(out.exit, 0, "{}", out.text);
    assert!(out.text.contains("team-a"));

    // Key is enrolled and owns the tenant.
    let user = alice.user.clone().expect("signup enrolls the key");
    assert_eq!(user.name, "alice");
    let reg = st.manager.registry();
    assert_eq!(
        reg.user_for_pubkey(&alice.fingerprint).unwrap(),
        Some(user.clone())
    );
    let t = reg.tenant_by_slug("team-a").unwrap().unwrap();
    assert_eq!(
        reg.role(user.id, &t.id).unwrap(),
        Some(cliban_tenancy::Role::Owner)
    );
}

#[test]
fn signup_token_policy_gates_on_the_token() {
    let st = state(SignupPolicy::Token, Some("sesame"));
    assert_eq!(
        commands::run(&st, &mut key("a"), "signup t1 sesame").exit,
        0
    );
    assert_eq!(commands::run(&st, &mut key("b"), "signup t2 wrong").exit, 1);
    assert_eq!(commands::run(&st, &mut key("c"), "signup t3").exit, 1);
}

#[test]
fn signup_token_policy_without_configured_token_denies_all() {
    // The config default: policy=token, token=None. Deny until configured.
    let st = state(SignupPolicy::Token, None);
    let out = commands::run(&st, &mut key("a"), "signup t1 anything");
    assert_eq!(out.exit, 1);
}

#[test]
fn signup_closed_denies() {
    let st = state(SignupPolicy::Closed, None);
    assert_eq!(commands::run(&st, &mut key("a"), "signup t1").exit, 1);
}

#[test]
fn signup_twice_reuses_the_user_and_maps_registry_errors() {
    let st = state(SignupPolicy::Open, None);
    let mut alice = key("alice");
    commands::run(&st, &mut alice, "signup team-a");
    let first = alice.user.clone().unwrap();

    // Second tenant, same key: same user, no duplicate enrollment.
    let out = commands::run(&st, &mut alice, "signup team-b");
    assert_eq!(out.exit, 0, "{}", out.text);
    assert_eq!(alice.user.clone().unwrap(), first);

    // Taken slug and invalid slug surface as command failures, not panics.
    let mut bob = key("bob");
    let taken = commands::run(&st, &mut bob, "signup team-a");
    assert_eq!(taken.exit, 1);
    assert!(taken.text.contains("taken"), "{}", taken.text);
    assert_eq!(commands::run(&st, &mut bob, "signup Bad_Slug").exit, 1);
}

#[test]
fn whoami_unknown_key_prints_guidance() {
    let st = state(SignupPolicy::Open, None);
    let mut k = key("drifter");
    let out = commands::run(&st, &mut k, "whoami");
    assert_eq!(out.exit, 0);
    assert!(out.text.contains("not enrolled"), "{}", out.text);
    assert!(out.text.contains("signup"), "{}", out.text);
}

#[test]
fn whoami_known_key_lists_memberships() {
    let st = state(SignupPolicy::Open, None);
    let mut alice = key("alice");
    commands::run(&st, &mut alice, "signup team-a");
    let out = commands::run(&st, &mut alice, "whoami");
    assert_eq!(out.exit, 0);
    assert!(out.text.contains("alice"));
    assert!(out.text.contains("team-a (owner)"), "{}", out.text);
}

#[test]
fn unknown_or_empty_commands_print_usage() {
    let st = state(SignupPolicy::Open, None);
    for cmd in ["", "  ", "frobnicate", "signup", "signup a b c"] {
        let out = commands::run(&st, &mut key("x"), cmd);
        assert_eq!(out.exit, 1, "cmd {cmd:?}");
        assert_eq!(out.text, USAGE, "cmd {cmd:?}");
    }
}

/// Mint an invite for `slug` as `alice`, return the code.
fn invite_code(st: &AppState, alice: &mut KeyInfo, slug: &str) -> String {
    let out = commands::run(st, alice, &format!("invite {slug}"));
    assert_eq!(out.exit, 0, "{}", out.text);
    // First line: "invite code for '<slug>': <code>"
    out.text
        .lines()
        .next()
        .unwrap()
        .rsplit(' ')
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn invite_accept_members_full_flow() {
    let st = state(SignupPolicy::Open, None);
    let mut alice = key("alice");
    commands::run(&st, &mut alice, "signup team-a");

    let code = invite_code(&st, &mut alice, "team-a");
    assert_eq!(code.len(), 32);

    // The full output format is load-bearing (parsed by e2e + humans).
    let out = commands::run(&st, &mut alice, "invite team-a");
    let lines: Vec<&str> = out.text.lines().collect();
    assert!(
        lines[0].starts_with("invite code for 'team-a': "),
        "{}",
        out.text
    );
    assert!(lines[1].starts_with("expires: "), "{}", out.text);
    let code2 = lines[0].rsplit(' ').next().unwrap();
    assert_eq!(
        lines[2],
        format!("redeem with: ssh <host> accept {code2}"),
        "{}",
        out.text
    );

    // Unknown key accepts: enrolled as member.
    let mut bob = key("bob");
    let out = commands::run(&st, &mut bob, &format!("accept {code}"));
    assert_eq!(out.exit, 0, "{}", out.text);
    assert!(out.text.contains("team-a"));
    assert!(bob.user.is_some());

    // Both parties see the same member list.
    for k in [&mut alice, &mut bob] {
        let out = commands::run(&st, k, "members team-a");
        assert_eq!(out.exit, 0, "{}", out.text);
        assert!(out.text.contains("alice (owner)"), "{}", out.text);
        assert!(out.text.contains("bob (member)"), "{}", out.text);
    }

    // The code was one-time.
    let mut carol = key("carol");
    assert_eq!(
        commands::run(&st, &mut carol, &format!("accept {code}")).exit,
        1
    );
}

#[test]
fn invite_is_owner_only_and_needs_enrollment() {
    let st = state(SignupPolicy::Open, None);
    let mut alice = key("alice");
    commands::run(&st, &mut alice, "signup team-a");
    let code = invite_code(&st, &mut alice, "team-a");

    let mut bob = key("bob");
    // Unknown key cannot invite.
    assert_eq!(commands::run(&st, &mut bob, "invite").exit, 1);
    commands::run(&st, &mut bob, &format!("accept {code}"));
    // Member (not owner) cannot invite.
    let out = commands::run(&st, &mut bob, "invite team-a");
    assert_eq!(out.exit, 1);
    assert!(out.text.contains("owner"), "{}", out.text);
}

#[test]
fn tenant_resolution_bare_vs_slug() {
    let st = state(SignupPolicy::Open, None);
    let mut alice = key("alice");
    commands::run(&st, &mut alice, "signup team-a");

    // Sole membership: bare `invite`/`members` resolve implicitly.
    assert_eq!(commands::run(&st, &mut alice, "invite").exit, 0);
    assert_eq!(commands::run(&st, &mut alice, "members").exit, 0);

    // Two memberships: bare forms are ambiguous and name the options.
    commands::run(&st, &mut alice, "signup team-b");
    let out = commands::run(&st, &mut alice, "invite");
    assert_eq!(out.exit, 1);
    assert!(
        out.text.contains("team-a") && out.text.contains("team-b"),
        "{}",
        out.text
    );

    // Slug you don't belong to.
    assert_eq!(commands::run(&st, &mut alice, "members team-zzz").exit, 1);

    // Unenrolled key gets guidance, not a crash.
    let mut nobody = key("nobody");
    assert_eq!(commands::run(&st, &mut nobody, "members").exit, 1);
}

#[test]
fn display_names_are_sanitized() {
    let st = state(SignupPolicy::Open, None);

    // Control chars, spaces, punctuation are stripped from the SSH username.
    let mut evil = key("x");
    evil.username = "e v!i@l#".to_string();
    let out = commands::run(&st, &mut evil, "signup team-e");
    assert_eq!(out.exit, 0, "{}", out.text);
    assert_eq!(evil.user.clone().unwrap().name, "evil");

    // A username with nothing salvageable falls back to "user".
    let mut blank = key("y");
    blank.username = "\u{1b}\u{7} ".to_string();
    commands::run(&st, &mut blank, "signup team-f");
    assert_eq!(blank.user.clone().unwrap().name, "user");
}

#[test]
fn accept_bad_code_fails_cleanly() {
    let st = state(SignupPolicy::Open, None);
    let mut bob = key("bob");
    let out = commands::run(&st, &mut bob, "accept deadbeef");
    assert_eq!(out.exit, 1);
    assert!(out.text.contains("invalid"), "{}", out.text);
}
