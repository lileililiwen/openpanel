use super::common::CliRunner;

#[tokio::test]
async fn cli_host_ssh_key_add_then_list_then_remove() {
    use base64::Engine;
    let runner = CliRunner::new().await;
    let user = runner.run(&[
        "user",
        "create",
        "--username",
        "admin",
        "--email",
        "admin@example.test",
        "--password",
        "correct horse battery staple",
        "--role",
        "owner",
    ]);
    assert_eq!(user.code, 0, "{}", user.stderr);
    let drop_ins = runner.workdir.join("ssh");
    std::fs::create_dir_all(&drop_ins).unwrap();
    let keys_path = drop_ins.join("authorized_keys");
    let env = [(
        "OPENPANEL__SECURITY__AUTHORIZED_KEYS",
        keys_path.to_str().unwrap(),
    )];

    let body = vec![0xABu8; 68];
    let key_line = format!(
        "ssh-ed25519 {} tester@host",
        base64::engine::general_purpose::STANDARD.encode(body)
    );

    // Add.
    let added = runner.run_with_env(
        &["ssh-keys", "add", "--label", "laptop", "--key", &key_line],
        &env,
    );
    assert_eq!(added.code, 0, "{}", added.stderr);
    let id = added
        .stdout
        .split_whitespace()
        .next()
        .expect("id")
        .to_owned();

    // List shows the fingerprint and label.
    let listed = runner.run_with_env(&["ssh-keys", "list"], &env);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert!(listed.stdout.contains("SHA256:"));
    assert!(listed.stdout.contains("laptop"));

    // The managed authorized_keys file was written with the label.
    let rendered = std::fs::read_to_string(drop_ins.join("authorized_keys")).expect("file read");
    assert!(rendered.contains("openpanel:laptop"));
    assert!(rendered.contains("# BEGIN openpanel-managed (do not edit)"));

    // Remove.
    let removed = runner.run_with_env(&["ssh-keys", "remove", "--id", &id], &env);
    assert_eq!(removed.code, 0, "{}", removed.stderr);
    let listed = runner.run_with_env(&["ssh-keys", "list"], &env);
    assert!(!listed.stdout.contains("laptop"));
}
