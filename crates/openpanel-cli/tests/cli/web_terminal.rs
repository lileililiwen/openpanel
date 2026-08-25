use openpanel_domain::SiteRepository;

use super::common::CliRunner;

#[tokio::test]
async fn cli_terminal_ticket_prints_64_hex_token() {
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
    let owner_id = sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE username = 'admin'")
        .fetch_one(&runner.db.pool())
        .await
        .expect("owner id");
    let site_id = uuid::Uuid::new_v4();
    let site = openpanel_domain::Site::new(
        site_id,
        uuid::Uuid::parse_str(&owner_id).expect("owner uuid"),
        "cli-terminal.example.test",
        vec![],
        "/var/www/cli-terminal.example.test/public_html",
        false,
        None,
        "admin",
    )
    .expect("site");
    openpanel_app::sites::repo::SqliteSiteRepository::new(runner.db.pool())
        .insert(&site)
        .await
        .expect("insert site");

    let ticket = runner.run(&["terminal", "ticket", "--site", &site_id.to_string()]);
    assert_eq!(ticket.code, 0, "{}", ticket.stderr);
    let token = ticket.stdout.trim();
    assert_eq!(token.len(), 64);
    assert!(
        token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );

    // Unknown site exits non-zero.
    let unknown = runner.run(&[
        "terminal",
        "ticket",
        "--site",
        &uuid::Uuid::new_v4().to_string(),
    ]);
    assert_ne!(unknown.code, 0);
}
