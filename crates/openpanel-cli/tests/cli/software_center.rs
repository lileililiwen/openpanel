use super::common::CliRunner;

#[tokio::test]
async fn cli_catalog_preview_install_and_jobs_use_fake_adapter() {
    let runner = CliRunner::new().await;
    let env = [("OPENPANEL__SOFTWARE__ADAPTER", "fake")];
    for args in [
        vec!["software", "catalog"],
        vec!["software", "inventory"],
        vec!["software", "diagnostics"],
        vec!["software", "preview", "--id", "redis"],
        vec!["software", "install", "--id", "redis"],
        vec![
            "software",
            "deploy",
            "--application",
            "wordpress",
            "--domain",
            "example.test",
            "--php-version",
            "8.3",
        ],
    ] {
        let result = runner.run_with_env(&args, &env);
        assert_eq!(result.code, 0, "{:?}: {}", args, result.stderr);
    }
    let preview = runner.run_with_env(&["software", "preview", "--id", "nginx"], &env);
    assert_eq!(preview.code, 0, "{}", preview.stderr);
    let preview: serde_json::Value = serde_json::from_str(&preview.stdout).unwrap();
    let digest = preview["plan"]["digest"].as_str().unwrap();
    let token = preview["confirmation_token"].as_str().unwrap();
    let executed = runner.run_with_env(
        &[
            "software",
            "execute",
            "--digest",
            digest,
            "--confirmation-token",
            token,
        ],
        &env,
    );
    assert_eq!(executed.code, 0, "{}", executed.stderr);
    assert!(executed.stdout.contains("succeeded"));
    let jobs = runner.run_with_env(&["software", "jobs"], &env);
    assert_eq!(jobs.code, 0, "{}", jobs.stderr);
    assert!(jobs.stdout.contains("succeeded"), "{}", jobs.stdout);
    assert!(!jobs.stdout.to_ascii_lowercase().contains("password"));
}

#[tokio::test]
async fn jobs_reconcile_abandoned_active_transactions_after_restart() {
    let runner = CliRunner::new().await;
    let env = [("OPENPANEL__SOFTWARE__ADAPTER", "fake")];
    let preview = runner.run_with_env(&["software", "preview", "--id", "redis"], &env);
    assert_eq!(preview.code, 0, "{}", preview.stderr);
    let preview: serde_json::Value = serde_json::from_str(&preview.stdout).unwrap();
    let digest = preview["plan"]["digest"].as_str().unwrap();
    let job_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO software_jobs(id,plan_digest,component_id,state,events_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
        .bind(job_id.to_string())
        .bind(digest)
        .bind("redis")
        .bind("running")
        .bind("[]")
        .bind("2026-01-01T00:00:00Z")
        .bind("2026-01-01T00:00:00Z")
        .execute(&runner.db.pool())
        .await
        .unwrap();
    let jobs = runner.run_with_env(&["software", "jobs"], &env);
    assert_eq!(jobs.code, 0, "{}", jobs.stderr);
    assert!(jobs.stdout.contains("interrupted"), "{}", jobs.stdout);
    let retry = runner.run_with_env(&["software", "retry", "--job", &job_id.to_string()], &env);
    assert_eq!(retry.code, 0, "{}", retry.stderr);
    assert!(retry.stdout.contains("confirmation_token"));
    let rollback = runner.run_with_env(
        &["software", "rollback", "--job", &job_id.to_string()],
        &env,
    );
    assert_eq!(rollback.code, 0, "{}", rollback.stderr);
    assert!(rollback.stdout.contains("rolled_back"));
}
