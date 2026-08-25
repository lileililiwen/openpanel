use super::common::CliRunner;
#[tokio::test]
async fn cli_mail_readiness_domain_mailbox_alias_quota_password_and_status() {
    let runner = CliRunner::new().await;
    let env = [("OPENPANEL__MAIL__ADAPTER", "fake")];
    for args in [
        vec!["mail", "readiness"],
        vec!["mail", "domain-add", "--name", "example.test"],
        vec!["mail", "domains"],
        vec![
            "mail",
            "mailbox-add",
            "--domain",
            "example.test",
            "--local",
            "alice",
            "--quota",
            "1048576",
            "--password",
            "cli-mailbox-secret",
        ],
        vec!["mail", "mailboxes", "--domain", "example.test"],
        vec![
            "mail",
            "alias-add",
            "--domain",
            "example.test",
            "--source",
            "info",
            "--destination",
            "alice@example.test",
        ],
        vec![
            "mail",
            "quota",
            "--address",
            "alice@example.test",
            "--bytes",
            "2097152",
        ],
        vec![
            "mail",
            "password",
            "--address",
            "alice@example.test",
            "--password",
            "rotated-mailbox-secret",
        ],
        vec!["mail", "status"],
    ] {
        let result = runner.run_with_env(&args, &env);
        assert_eq!(result.code, 0, "{:?}: {}", args, result.stderr);
        if args.get(1) != Some(&"mailbox-add") && args.get(1) != Some(&"password") {
            assert!(!result.stdout.contains("cli-mailbox-secret"));
            assert!(!result.stdout.contains("rotated-mailbox-secret"));
        }
    }
}

#[tokio::test]
async fn cli_mail_queue_prints_snapshot() {
    let runner = CliRunner::new().await;
    let queue = runner.run(&["mail", "queue"]);
    assert_eq!(queue.code, 0, "{}", queue.stderr);
    assert!(queue.stdout.contains("\"queue_depth\""));
    assert!(
        queue.stdout.contains("\"health\""),
        "expected health field: {}",
        queue.stdout
    );
}
