use super::common::CliRunner;

#[tokio::test]
async fn cli_runtime_env_set_secret_from_stdin() {
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
    let runtime_id = uuid::Uuid::new_v4().to_string();

    // Secret arrives via stdin; the secret marker flags it. The value
    // never appears in argv.
    let mut child = std::process::Command::new(&runner.bin)
        .args([
            "runtime-env",
            "set",
            "--runtime",
            &runtime_id,
            "--secret-marker",
            "secret",
        ])
        .envs(runner.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"DATABASE_URL=postgres://s3cret secret\nLOG_LEVEL=debug\n")
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("environment saved"));
    assert!(!stdout.contains("s3cret"), "secret leaked to stdout");

    // Show masks the secret and round-trips the plain value.
    let show = runner.run(&["runtime-env", "show", "--runtime", &runtime_id]);
    assert_eq!(show.code, 0, "{}", show.stderr);
    let parsed: serde_json::Value = serde_json::from_str(show.stdout.trim()).expect("json");
    assert_eq!(parsed.as_array().expect("vars").len(), 2);
    for var in parsed.as_array().unwrap() {
        match var["key"].as_str().unwrap() {
            "DATABASE_URL" => {
                assert_eq!(var["secret"], serde_json::json!(true));
                assert!(var["value"].is_null(), "secret value leaked");
            }
            "LOG_LEVEL" => assert_eq!(var["value"], serde_json::json!("debug")),
            other => panic!("unexpected key {other}"),
        }
    }
}
