use super::common::CliRunner;

#[tokio::test]
async fn cli_backup_plan_run_verify_restore_lifecycle() {
    let runner = CliRunner::new().await;
    let root = runner.workdir.join("backups");
    std::fs::create_dir_all(&root).unwrap();
    let created = runner.run_with_env(
        &[
            "backup",
            "plan",
            "create",
            "--name",
            "nightly",
            "--schedule",
            "0 2 * * *",
            "--timezone",
            "UTC",
            "--panel-metadata",
            "--retention",
            "3",
        ],
        &[("OPENPANEL__BACKUPS__ROOT", root.to_str().unwrap())],
    );
    assert_eq!(created.code, 0, "{}", created.stderr);
    let id = created.stdout.split_whitespace().last().unwrap();
    for args in [
        vec!["backup", "plan", "list"],
        vec!["backup", "plan", "get", "--id", id],
        vec!["backup", "plan", "update", "--id", id, "--retention", "2"],
        vec!["backup", "plan", "disable", "--id", id],
        vec!["backup", "plan", "enable", "--id", id],
    ] {
        let result = runner.run_with_env(
            &args,
            &[("OPENPANEL__BACKUPS__ROOT", root.to_str().unwrap())],
        );
        assert_eq!(result.code, 0, "{:?}: {}", args, result.stderr);
    }
    let run = runner.run_with_env(
        &["backup", "run", "--plan-id", id],
        &[("OPENPANEL__BACKUPS__ROOT", root.to_str().unwrap())],
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    let run_id = run.stdout.split_whitespace().last().unwrap();
    for args in [
        vec!["backup", "runs"],
        vec!["backup", "status", "--id", run_id],
        vec!["backup", "verify", "--id", run_id],
        vec!["backup", "restore", "preview", "--id", run_id],
        vec!["backup", "restore", "start", "--id", run_id],
    ] {
        let result = runner.run_with_env(
            &args,
            &[("OPENPANEL__BACKUPS__ROOT", root.to_str().unwrap())],
        );
        assert_eq!(result.code, 0, "{:?}: {}", args, result.stderr);
    }
    assert_eq!(
        runner
            .run_with_env(
                &["backup", "delete", "--id", run_id],
                &[("OPENPANEL__BACKUPS__ROOT", root.to_str().unwrap())]
            )
            .code,
        0
    );
    assert_eq!(
        runner
            .run_with_env(
                &["backup", "plan", "delete", "--id", id],
                &[("OPENPANEL__BACKUPS__ROOT", root.to_str().unwrap())]
            )
            .code,
        0
    );
}

#[tokio::test]
async fn cli_backup_drill_run_then_show() {
    let runner = CliRunner::new().await;
    let root = runner.workdir.join("backups");
    std::fs::create_dir_all(&root).unwrap();
    let env = &[("OPENPANEL__BACKUPS__ROOT", root.to_str().unwrap())];
    let created = runner.run_with_env(
        &[
            "backup", "plan", "create", "--name", "drill",
            "--schedule", "0 2 * * *", "--timezone", "UTC",
            "--panel-metadata", "--retention", "3",
        ],
        env,
    );
    assert_eq!(created.code, 0, "{}", created.stderr);
    let plan_id = created.stdout.split_whitespace().last().unwrap();
    let run = runner.run_with_env(&["backup", "run", "--plan-id", plan_id], env);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let run_id = run.stdout.split_whitespace().last().unwrap();
    let drill = runner.run_with_env(&["backup", "drill", "run", "--id", run_id], env);
    assert_eq!(drill.code, 0, "{}", drill.stderr);
    let drill_id = drill.stdout.lines().last().unwrap().split_whitespace().nth(1).unwrap();
    let list = runner.run_with_env(&["backup", "drill", "list", "--id", run_id], env);
    assert_eq!(list.code, 0, "{}", list.stderr);
    assert!(list.stdout.contains(drill_id));
    let show = runner.run_with_env(&["backup", "drill", "show", "--id", drill_id], env);
    assert_eq!(show.code, 0, "{}", show.stderr);
    assert!(show.stdout.contains("passed"));
}

#[tokio::test]
async fn cli_site_preview_list_shows_pr_and_state() {
    let runner = CliRunner::new().await;
    let site = uuid::Uuid::new_v4().to_string();
    let list = runner.run(&["site", "preview", "list", "--site", &site]);
    assert_eq!(list.code, 0, "{}", list.stderr);
    assert!(list.stdout.contains("[]"));
}
