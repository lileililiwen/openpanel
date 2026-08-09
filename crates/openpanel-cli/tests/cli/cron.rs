use super::common::CliRunner;

#[tokio::test]
async fn cli_cron_full_lifecycle() {
    let runner = CliRunner::new().await;
    let working = runner.workdir.join("site");
    std::fs::create_dir_all(&working).expect("working directory");
    let working = working.to_string_lossy();

    let create = runner.run(&[
        "cron",
        "create",
        "--name",
        "echo",
        "--schedule",
        "*/5 * * * *",
        "--timezone",
        "UTC",
        "--executable",
        "/bin/echo",
        "--arg",
        "hello",
        "--working-directory",
        &working,
        "--timeout",
        "60",
    ]);
    assert_eq!(create.code, 0, "{}", create.stderr);
    let id = create
        .stdout
        .split_whitespace()
        .last()
        .expect("created job id");

    for args in [
        vec!["cron", "list"],
        vec!["cron", "get", "--id", id],
        vec!["cron", "update", "--id", id, "--schedule", "0 * * * *"],
        vec!["cron", "disable", "--id", id],
        vec!["cron", "enable", "--id", id],
        vec!["cron", "run", "--id", id],
        vec!["cron", "runs", "--job-id", id],
    ] {
        let result = runner.run(&args);
        assert_eq!(result.code, 0, "args={args:?}: {}", result.stderr);
    }

    let delete = runner.run(&["cron", "delete", "--id", id]);
    assert_eq!(delete.code, 0, "{}", delete.stderr);
}
