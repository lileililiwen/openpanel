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
