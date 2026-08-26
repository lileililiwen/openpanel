use super::common::CliRunner;

#[tokio::test]
async fn cli_logs_sources_tail_errors_traffic_audit_and_export() {
    let runner = CliRunner::new().await;
    let root = runner.workdir.join("logs");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("panel-error.log"),
        "2026-08-09T02:00:00Z ERROR failed /login?password=secret\n",
    )
    .unwrap();
    let root_value = root.to_str().unwrap();
    let sources = runner.run_with_env(
        &["logs", "sources"],
        &[("OPENPANEL__LOGS__ROOT", root_value)],
    );
    assert_eq!(sources.code, 0, "{}", sources.stderr);
    assert!(sources.stdout.contains("panel-error"));

    for args in [
        vec!["logs", "tail", "--source", "panel-error", "--limit", "10"],
        vec!["logs", "errors", "--limit", "10"],
        vec!["logs", "traffic"],
        vec!["logs", "audit"],
    ] {
        let result = runner.run_with_env(&args, &[("OPENPANEL__LOGS__ROOT", root_value)]);
        assert_eq!(result.code, 0, "{:?}: {}", args, result.stderr);
        assert!(!result.stdout.contains("secret"));
    }

    let destination = runner.workdir.join("export.log");
    let export = runner.run_with_env(
        &[
            "logs",
            "export",
            "--source",
            "panel-error",
            "--output",
            destination.to_str().unwrap(),
        ],
        &[("OPENPANEL__LOGS__ROOT", root_value)],
    );
    assert_eq!(export.code, 0, "{}", export.stderr);
    assert!(
        !std::fs::read_to_string(destination)
            .unwrap()
            .contains("secret")
    );
}

#[tokio::test]
async fn cli_logs_policy_set_then_show() {
    let runner = CliRunner::new().await;
    let drop_ins = runner.workdir.join("logrotate.d");
    std::fs::create_dir_all(&drop_ins).unwrap();
    let env = [("OPENPANEL__LOGS__DROP_IN_ROOT", drop_ins.to_str().unwrap())];
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

    // Show before any policy exists is refused.
    let unset = runner.run_with_env(&["logs", "policy", "show", "--class", "panel"], &env);
    assert_ne!(unset.code, 0);

    // Set a valid policy.
    let set = runner.run_with_env(
        &[
            "logs",
            "policy",
            "set",
            "--class",
            "panel",
            "--max-age-days",
            "30",
            "--max-size-mb",
            "100",
            "--keep-generations",
            "4",
            "--compress",
        ],
        &env,
    );
    assert_eq!(set.code, 0, "{}", set.stderr);
    assert!(set.stdout.contains("rotation policy saved for panel"));

    // Show round-trips the stored values.
    let show = runner.run_with_env(&["logs", "policy", "show", "--class", "panel"], &env);
    assert_eq!(show.code, 0, "{}", show.stderr);
    let parsed: serde_json::Value = serde_json::from_str(show.stdout.trim()).expect("json");
    assert_eq!(parsed["policy"]["max_age_days"], serde_json::json!(30));
    assert_eq!(parsed["drift"], serde_json::json!(false));

    // Invalid bounds are refused.
    let invalid = runner.run_with_env(
        &[
            "logs",
            "policy",
            "set",
            "--class",
            "panel",
            "--max-age-days",
            "366",
            "--max-size-mb",
            "100",
            "--keep-generations",
            "4",
        ],
        &env,
    );
    assert_ne!(invalid.code, 0);
}
