use cliban_server::hostkey;

#[test]
fn generates_then_reloads_same_key() {
    let dir = std::env::temp_dir().join(format!("cliband_hostkey_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    // First boot: dir does not exist yet — must be created, key generated.
    let first = hostkey::load_or_generate(&dir).unwrap();
    let path = dir.join(hostkey::HOST_KEY_FILE);
    assert!(path.is_file());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    // Second boot: the same key comes back.
    let second = hostkey::load_or_generate(&dir).unwrap();
    assert_eq!(
        first.public_key().to_openssh().unwrap(),
        second.public_key().to_openssh().unwrap()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
