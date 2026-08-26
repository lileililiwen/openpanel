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

#[tokio::test]
async fn cli_mail_autoresponder_set_and_show() {
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

    let env = [("OPENPANEL__MAIL__ADAPTER", "fake")];
    let domain = runner.run_with_env(&["mail", "domain-add", "--name", "auto.example.test"], &env);
    assert_eq!(domain.code, 0, "{}", domain.stderr);
    let mailbox = runner.run_with_env(
        &[
            "mail",
            "mailbox-add",
            "--domain",
            "auto.example.test",
            "--local",
            "bob",
            "--quota",
            "1048576",
            "--password",
            "cli-autoresponder-secret",
        ],
        &env,
    );
    assert_eq!(mailbox.code, 0, "{}", mailbox.stderr);
    let mailbox_id: String =
        sqlx::query_scalar("SELECT id FROM mail_mailboxes WHERE address = 'bob@auto.example.test'")
            .fetch_one(&runner.db.pool())
            .await
            .expect("mailbox id");
    let mailbox_id = mailbox_id.as_str();

    // Set.
    let set = runner.run(&[
        "mail",
        "autoresponder",
        "--mailbox",
        mailbox_id,
        "--body",
        "I am out of office until Monday",
    ]);
    assert_eq!(set.code, 0, "{}", set.stderr);
    assert!(set.stdout.contains("autoresponder enabled"));

    // Show.
    let show = runner.run(&["mail", "autoresponder", "--mailbox", mailbox_id]);
    assert_eq!(show.code, 0, "{}", show.stderr);
    assert!(show.stdout.contains("\"enabled\": true"), "{}", show.stdout);
    assert!(
        show.stdout.contains("I am out of office until Monday"),
        "{}",
        show.stdout
    );

    // Disable, then show again — body kept, enabled flipped.
    let off = runner.run(&["mail", "autoresponder", "--mailbox", mailbox_id, "--off"]);
    assert_eq!(off.code, 0, "{}", off.stderr);
    assert!(off.stdout.contains("autoresponder disabled"));
    let show_off = runner.run(&["mail", "autoresponder", "--mailbox", mailbox_id]);
    assert_eq!(show_off.code, 0, "{}", show_off.stderr);
    assert!(
        show_off.stdout.contains("\"enabled\": false"),
        "{}",
        show_off.stdout
    );
}
