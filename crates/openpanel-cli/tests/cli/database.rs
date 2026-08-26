//! E2E test: `openpanel database` create roundtrip.
//!
//! Provisioning a MySQL database shells out to the `mysql` CLI, so this
//! test is skipped when mysql is not installed.

use std::process::Command;

use super::common::CliRunner;

fn mysql_available() -> bool {
    let which = Command::new("which").arg("mysql").output();
    matches!(which, Ok(o) if o.status.success())
}

#[tokio::test]
async fn cli_database_create_and_list() {
    if !mysql_available() {
        eprintln!("skipping cli_database_create_and_list: mysql not installed");
        return;
    }

    let runner = CliRunner::new().await;

    // The databases module needs a master key; serve boots it eagerly.
    let master_key = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([0u8; 32])
    };

    let user = runner.run(&[
        "user",
        "create",
        "--username",
        "admin",
        "--email",
        "admin@example.com",
        "--password",
        "correct horse battery staple",
        "--role",
        "owner",
    ]);
    assert_eq!(user.code, 0, "user create failed: {}", user.stderr);

    let create = runner.run_with_env(
        &["database", "create", "--owner", "admin", "--suffix", "blog"],
        &[("OPENPANEL__DATABASE__MASTER_KEY", &master_key)],
    );
    assert_eq!(create.code, 0, "database create failed: {}", create.stderr);
    assert!(
        create.stdout.contains("created database"),
        "create output should confirm, got: {}",
        create.stdout
    );

    let list = runner.run_with_env(
        &["database", "list"],
        &[("OPENPANEL__DATABASE__MASTER_KEY", &master_key)],
    );
    assert_eq!(list.code, 0, "database list failed: {}", list.stderr);
}

#[tokio::test]
async fn cli_database_remote_access_add_then_show() {
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
    let drop_ins = runner.workdir.join("db-remote");
    std::fs::create_dir_all(&drop_ins).unwrap();
    let env = [
        ("OPENPANEL__DATABASES__GRANT_PORT", "memory"),
        ("OPENPANEL__LOGS__DROP_IN_ROOT", drop_ins.to_str().unwrap()),
    ];
    let db_id = uuid::Uuid::new_v4().to_string();

    // Add an ACL.
    let added = runner.run_with_env(
        &[
            "database",
            "remote-access",
            "add",
            "--database",
            &db_id,
            "--user",
            "wp1",
            "--name",
            "wp1db",
            "--cidrs",
            "203.0.113.0/24",
        ],
        &env,
    );
    assert_eq!(added.code, 0, "{}", added.stderr);
    assert!(added.stdout.contains("remote access enabled for wp1"));

    // Show round-trips the stored ACL.
    let show = runner.run_with_env(
        &["database", "remote-access", "show", "--database", &db_id],
        &env,
    );
    assert_eq!(show.code, 0, "{}", show.stderr);
    let parsed: serde_json::Value = serde_json::from_str(show.stdout.trim()).expect("json");
    assert_eq!(parsed["enabled"], serde_json::json!(true));
    assert_eq!(parsed["allow_cidrs"], serde_json::json!(["203.0.113.0/24"]));

    // Wildcard without opt-in is refused.
    let locked = runner.run_with_env(
        &[
            "database",
            "remote-access",
            "add",
            "--database",
            &db_id,
            "--user",
            "wp1",
            "--name",
            "wp1db",
            "--cidrs",
            "0.0.0.0/0",
        ],
        &env,
    );
    assert_ne!(locked.code, 0);
}
