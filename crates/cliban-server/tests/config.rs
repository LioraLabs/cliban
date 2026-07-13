use cliban_server::config::{ServerConfig, SignupPolicy};

#[test]
fn defaults_are_sane() {
    let c = ServerConfig::default();
    assert_eq!(c.listen_addr, "0.0.0.0:2222");
    assert!(c.data_dir.ends_with("cliband"));
    assert_eq!(c.signup_policy, SignupPolicy::Token);
    assert_eq!(c.signup_token, None);
    assert_eq!(c.max_tenants_per_key, 5);
    assert_eq!(c.max_tenants, 0); // 0 = unlimited
}

#[test]
fn parses_full_toml() {
    let c = ServerConfig::parse(
        r#"
listen_addr = "127.0.0.1:2022"
data_dir = "/var/lib/cliband"
signup_policy = "open"
signup_token = "s3cret"
max_tenants_per_key = 2
max_tenants = 50
"#,
    )
    .unwrap();
    assert_eq!(c.listen_addr, "127.0.0.1:2022");
    assert_eq!(c.data_dir, std::path::PathBuf::from("/var/lib/cliband"));
    assert_eq!(c.signup_policy, SignupPolicy::Open);
    assert_eq!(c.signup_token.as_deref(), Some("s3cret"));
    assert_eq!(c.max_tenants_per_key, 2);
    assert_eq!(c.max_tenants, 50);
}

#[test]
fn partial_toml_keeps_defaults() {
    let c = ServerConfig::parse("listen_addr = \"127.0.0.1:9\"\n").unwrap();
    assert_eq!(c.listen_addr, "127.0.0.1:9");
    assert_eq!(c.signup_policy, SignupPolicy::Token);
    assert_eq!(c.max_tenants_per_key, 5);
}

#[test]
fn rejects_unknown_policy() {
    assert!(ServerConfig::parse("signup_policy = \"invite\"\n").is_err());
}

#[test]
fn rejects_unknown_keys() {
    assert!(ServerConfig::parse("listen_adr = \"typo\"\n").is_err());
}

#[test]
fn load_errors_on_missing_file() {
    let missing = std::env::temp_dir().join(format!("cliband_no_such_{}.toml", std::process::id()));
    assert!(ServerConfig::load(&missing).is_err());
}

#[test]
fn load_reads_a_file() {
    let path = std::env::temp_dir().join(format!("cliband_cfg_{}.toml", std::process::id()));
    std::fs::write(&path, "signup_policy = \"closed\"\n").unwrap();
    let c = ServerConfig::load(&path).unwrap();
    assert_eq!(c.signup_policy, SignupPolicy::Closed);
    let _ = std::fs::remove_file(&path);
}
