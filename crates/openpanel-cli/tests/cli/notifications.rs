//! Notification CLI lifecycle against isolated SQLite.

use super::common::CliRunner;

#[tokio::test]
async fn cli_notification_channel_and_subscription_lifecycle() {
    let runner = CliRunner::new().await;
    let user = runner.run(&[
        "user",
        "create",
        "--username",
        "owner",
        "--email",
        "owner@example.test",
        "--password",
        "correct horse battery staple",
        "--role",
        "owner",
    ]);
    assert_eq!(user.code, 0, "{}", user.stderr);
    let created = runner.run(&[
        "notifications",
        "channel",
        "add",
        "--kind",
        "smtp",
        "--name",
        "relay",
        "--endpoint",
        "smtp.example.test",
        "--username",
        "mailer",
        "--credential",
        "secret-password",
        "--from-addr",
        "sender@example.test",
        "--allow",
        "ops@example.test",
    ]);
    assert_eq!(created.code, 0, "{}", created.stderr);
    assert!(!created.stdout.contains("secret-password"));
    let channel: serde_json::Value = serde_json::from_str(&created.stdout).unwrap();
    let id = channel["id"].as_str().unwrap();
    let subscription = runner.run(&[
        "notifications",
        "subscription",
        "add",
        "--channel",
        id,
        "--destination",
        "ops@example.test",
        "--kind",
        "alert",
        "--filter-json",
        r#"{"metrics":["cpu_percent"]}"#,
    ]);
    assert_eq!(subscription.code, 0, "{}", subscription.stderr);
    let subscription: serde_json::Value = serde_json::from_str(&subscription.stdout).unwrap();
    let subscription_id = subscription["id"].as_str().unwrap();
    let listed = runner.run(&["notifications", "channel", "list"]);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert!(!listed.stdout.contains("secret-password"));
    assert_eq!(
        runner
            .run(&[
                "notifications",
                "subscription",
                "rm",
                "--id",
                subscription_id
            ])
            .code,
        0
    );
    assert_eq!(
        runner
            .run(&["notifications", "channel", "rm", "--id", id])
            .code,
        0
    );
}
