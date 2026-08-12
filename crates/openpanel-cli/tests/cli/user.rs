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

#[tokio::test]
async fn cli_user_two_factor_enroll_list_revoke_and_regenerate() {
    let runner = CliRunner::new().await;
    let create = runner.run(&[
        "user",
        "create",
        "--username",
        "secured",
        "--email",
        "secured@example.com",
        "--password",
        "correct horse battery staple",
        "--role",
        "owner",
    ]);
    assert_eq!(create.code, 0, "user create failed: {}", create.stderr);
    let user_id = create
        .stdout
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(id, _)| id.to_string())
        .expect("created user id");

    let enroll = runner.run(&["user", "2fa", "enroll", "totp", "--id", &user_id]);
    assert_eq!(enroll.code, 0, "2FA enroll failed: {}", enroll.stderr);
    assert!(enroll.stdout.contains("secret_base32:"));
    assert!(
        enroll
            .stdout
            .contains("recovery_codes (single-use, shown once):")
    );
    let factor_id = enroll
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("factor: "))
        .map(str::to_string)
        .expect("enrolled factor id");

    let list = runner.run(&["user", "2fa", "list", "--id", &user_id]);
    assert_eq!(list.code, 0, "2FA list failed: {}", list.stderr);
    assert!(list.stdout.contains(&factor_id));
    assert!(list.stdout.contains("kind=totp"));
    assert!(list.stdout.contains("recovery_codes_remaining: 10"));

    let regenerate = runner.run(&["user", "2fa", "recovery", "regenerate", "--id", &user_id]);
    assert_eq!(
        regenerate.code, 0,
        "recovery regenerate failed: {}",
        regenerate.stderr
    );
    assert_eq!(
        regenerate
            .stdout
            .lines()
            .filter(|line| line.starts_with("  "))
            .count(),
        10
    );

    let revoke = runner.run(&[
        "user",
        "2fa",
        "revoke",
        "--id",
        &user_id,
        "--factor-id",
        &factor_id,
    ]);
    assert_eq!(revoke.code, 0, "2FA revoke failed: {}", revoke.stderr);

    let list = runner.run(&["user", "2fa", "list", "--id", &user_id]);
    assert_eq!(list.code, 0, "2FA relist failed: {}", list.stderr);
    assert!(list.stdout.contains("(revoked)"));
}
