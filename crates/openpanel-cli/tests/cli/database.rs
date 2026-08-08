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
