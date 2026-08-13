use openpanel_domain::SiteRepository;

use super::common::CliRunner;

#[tokio::test]
async fn cli_waf_rules_add_disable_enable_test_and_remove() {
    let mut runner = CliRunner::new().await;
    runner.env.push((
        "OPENPANEL_WAF_NGINX_ROOT".into(),
        runner.workdir.join("nginx").display().to_string(),
    ));
    runner
        .env
        .push(("OPENPANEL_WAF_NGINX_BINARY".into(), "/bin/true".into()));
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
        "cli-waf.example.test",
        vec![],
        "/var/www/cli-waf.example.test/public_html",
        false,
        None,
        "admin",
    )
    .expect("site");
    openpanel_app::sites::repo::SqliteSiteRepository::new(runner.db.pool())
        .insert(&site)
        .await
        .expect("insert site");
    let site_id = site_id.to_string();
    let rule_id = uuid::Uuid::new_v4().to_string();
    let rule = format!(
        "{{\"kind\":\"path_block\",\"id\":\"{rule_id}\",\"enabled\":true,\"priority\":10,\"pattern\":\"/private\",\"method\":null,\"action\":\"deny\"}}"
    );
    let add = runner.run(&["waf", "add", "--site", &site_id, "--rule-json", &rule]);
    assert_eq!(add.code, 0, "{}", add.stderr);
    let list = runner.run(&["waf", "rules", "--site", &site_id]);
    assert_eq!(list.code, 0, "{}", list.stderr);
    assert!(list.stdout.contains(&rule_id));
    let hits = runner.run(&["waf", "hits", "--site", &site_id]);
    assert_eq!(hits.code, 0, "{}", hits.stderr);
    assert_eq!(hits.stdout.trim(), "[]");
    for action in ["disable", "enable"] {
        let result = runner.run(&["waf", action, "--site", &site_id, "--rule-id", &rule_id]);
        assert_eq!(result.code, 0, "{}: {}", action, result.stderr);
    }
    let request = r#"{"path":"/private/key","method":"GET","user_agent":"Browser","country":"US"}"#;
    let dry = runner.run(&[
        "waf",
        "test",
        "--site",
        &site_id,
        "--rule-json",
        &rule,
        "--request-json",
        request,
    ]);
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    assert!(dry.stdout.contains("\"would_match\": true"));
    let remove = runner.run(&["waf", "remove", "--site", &site_id, "--rule-id", &rule_id]);
    assert_eq!(remove.code, 0, "{}", remove.stderr);
    let list = runner.run(&["waf", "rules", "--site", &site_id]);
    assert_eq!(list.code, 0, "{}", list.stderr);
    assert!(!list.stdout.contains(&rule_id));
}
