use super::common::CliRunner;

#[tokio::test]
async fn cli_server_snapshot_create_then_list() {
    let runner = CliRunner::new().await;
    let root = runner.workdir.join("backups");
    std::fs::create_dir_all(&root).unwrap();
    let snapshots = runner.workdir.join("snapshots");
    std::fs::create_dir_all(&snapshots).unwrap();
    let env = [
        ("OPENPANEL__BACKUPS__ROOT", root.to_str().unwrap()),
        ("OPENPANEL__SNAPSHOTS__ROOT", snapshots.to_str().unwrap()),
    ];

    // Seed a completed backup run through the backup CLI.
    let created = runner.run_with_env(
        &[
            "backup",
            "plan",
            "create",
            "--name",
            "snapshot-source",
            "--schedule",
            "0 2 * * *",
            "--timezone",
            "UTC",
            "--panel-metadata",
            "--retention",
            "3",
        ],
        &env,
    );
    assert_eq!(created.code, 0, "{}", created.stderr);
    let plan_id = created.stdout.split_whitespace().last().unwrap().to_owned();
    let run = runner.run_with_env(&["backup", "run", "--plan-id", &plan_id], &env);
    assert_eq!(run.code, 0, "{}", run.stderr);

    // Create the snapshot from the latest completed run.
    let snapshot = runner.run_with_env(&["server-snapshot", "create"], &env);
    assert_eq!(snapshot.code, 0, "{}", snapshot.stderr);
    assert!(snapshot.stdout.contains("created snapshot with"));

    // List shows it with an id and entry count.
    let listed = runner.run_with_env(&["server-snapshot", "list"], &env);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    let first_line = listed
        .stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("snapshot line");
    let snapshot_id = first_line.split_whitespace().next().expect("id").to_owned();
    uuid::Uuid::parse_str(&snapshot_id).expect("snapshot id is a uuid");

    // Get renders manifest details.
    let got = runner.run_with_env(&["server-snapshot", "get", "--id", &snapshot_id], &env);
    assert_eq!(got.code, 0, "{}", got.stderr);
    assert!(got.stdout.contains("\"entries\""));
}

#[tokio::test]
async fn cli_server_restore_requires_confirm() {
    let runner = CliRunner::new().await;
    let root = runner.workdir.join("backups");
    std::fs::create_dir_all(&root).unwrap();
    let snapshots = runner.workdir.join("snapshots");
    std::fs::create_dir_all(&snapshots).unwrap();
    let env = [
        ("OPENPANEL__BACKUPS__ROOT", root.to_str().unwrap()),
        ("OPENPANEL__SNAPSHOTS__ROOT", snapshots.to_str().unwrap()),
    ];

    let created = runner.run_with_env(
        &[
            "backup",
            "plan",
            "create",
            "--name",
            "restore-source",
            "--schedule",
            "0 2 * * *",
            "--timezone",
            "UTC",
            "--panel-metadata",
            "--retention",
            "3",
        ],
        &env,
    );
    assert_eq!(created.code, 0, "{}", created.stderr);
    let plan_id = created.stdout.split_whitespace().last().unwrap().to_owned();
    let run = runner.run_with_env(&["backup", "run", "--plan-id", &plan_id], &env);
    assert_eq!(run.code, 0, "{}", run.stderr);
    runner
        .run_with_env(&["server-snapshot", "create"], &env)
        .code
        .eq(&0)
        .then_some(())
        .expect("snapshot create");

    let listed = runner.run_with_env(&["server-snapshot", "list"], &env);
    let snapshot_id = listed
        .stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("snapshot line")
        .split_whitespace()
        .next()
        .expect("id")
        .to_owned();

    // Restore without preflight confirmation is refused.
    let no_token = runner.run_with_env(
        &[
            "server-snapshot",
            "restore",
            "--id",
            &snapshot_id,
            "--confirm",
            "",
        ],
        &env,
    );
    assert_ne!(no_token.code, 0);

    // A wrong token is refused too.
    let wrong_token = runner.run_with_env(
        &[
            "server-snapshot",
            "restore",
            "--id",
            &snapshot_id,
            "--confirm",
            "a".repeat(32).as_str(),
        ],
        &env,
    );
    assert_ne!(wrong_token.code, 0);

    // Preflight mints the single-use token that unlocks restore.
    let preflight = runner.run_with_env(
        &["server-snapshot", "preflight", "--id", &snapshot_id],
        &env,
    );
    assert_eq!(preflight.code, 0, "{}", preflight.stderr);
    let parsed: serde_json::Value = serde_json::from_str(preflight.stdout.trim()).expect("json");
    assert_eq!(parsed["ready"], serde_json::json!(true));
    let confirm = parsed["confirm_token"].as_str().expect("confirm token");
    assert_eq!(confirm.len(), 32);
}
