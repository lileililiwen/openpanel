//! E2E test: `openpanel user` create + list roundtrip.

use super::common::CliRunner;

#[tokio::test]
async fn cli_user_create_and_list() {
    let runner = CliRunner::new().await;

    let create = runner.run(&[
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
    assert_eq!(create.code, 0, "user create failed: {}", create.stderr);
    assert!(
        create.stdout.contains("admin"),
        "create output should mention the user, got: {}",
        create.stdout
    );

    let list = runner.run(&["user", "list"]);
    assert_eq!(list.code, 0, "user list failed: {}", list.stderr);
    assert!(
        list.stdout.contains("admin"),
        "list should contain admin, got: {}",
        list.stdout
    );
}
