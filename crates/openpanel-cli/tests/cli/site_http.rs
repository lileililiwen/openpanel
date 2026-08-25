use openpanel_domain::SiteRepository;

use super::common::CliRunner;

#[tokio::test]
async fn cli_site_http_show_then_set_round_trips() {
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
        "cli-http-controls.example.test",
        vec![],
        "/var/www/cli-http-controls.example.test/public_html",
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

    // Initial document is empty.
    let show = runner.run(&["site-http", "show", "--site", &site_id]);
    assert_eq!(show.code, 0, "{}", show.stderr);
    assert!(show.stdout.contains("\"error_pages\": []"));

    // Set a document with one redirect and one error page.
    let controls = format!(
        "{{\"site_id\":\"{site_id}\",\"version\":1,\
          \"error_pages\":[{{\"status\":404,\"document_path\":\"/errors/404.html\"}}],\
          \"redirects\":[{{\"ordinal\":1,\"source_prefix\":\"/old\",\"destination\":\"/new\",\
          \"status\":\"moved_permanently\"}}],\
          \"protected_dirs\":[],\"hotlink\":null,\"ip_rules\":[],\"mime_overrides\":[],\
          \"index_policy\":null}}"
    );
    let set = runner.run(&[
        "site-http",
        "set",
        "--site",
        &site_id,
        "--controls-json",
        &controls,
    ]);
    assert_eq!(set.code, 0, "{}", set.stderr);

    let show = runner.run(&["site-http", "show", "--site", &site_id]);
    assert_eq!(show.code, 0, "{}", show.stderr);
    assert!(show.stdout.contains("/errors/404.html"));
    assert!(show.stdout.contains("/old"));

    // A redirect loop is rejected with a non-zero exit.
    let looping = format!(
        "{{\"site_id\":\"{site_id}\",\"version\":2,\
          \"error_pages\":[],\
          \"redirects\":[{{\"ordinal\":1,\"source_prefix\":\"/a\",\"destination\":\"/b\",\
          \"status\":\"moved_permanently\"}},{{\"ordinal\":2,\"source_prefix\":\"/b\",\
          \"destination\":\"/c\",\"status\":\"moved_permanently\"}}],\
          \"protected_dirs\":[],\"hotlink\":null,\"ip_rules\":[],\"mime_overrides\":[],\
          \"index_policy\":null}}"
    );
    let rejected = runner.run(&[
        "site-http",
        "set",
        "--site",
        &site_id,
        "--controls-json",
        &looping,
    ]);
    assert_ne!(rejected.code, 0);
}
