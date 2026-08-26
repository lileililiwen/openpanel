//! E2E test: `openpanel site` create + list roundtrip.
//!
//! Creating a site renders and writes an nginx config, so this test is
//! skipped when nginx is not installed (e.g. minimal dev machines).

use std::process::Command;

use super::common::CliRunner;

fn nginx_available() -> bool {
    let which = Command::new("which").arg("nginx").output();
    matches!(which, Ok(o) if o.status.success())
}

#[tokio::test]
async fn cli_site_create_and_list() {
    if !nginx_available() {
        eprintln!("skipping cli_site_create_and_list: nginx not installed");
        return;
    }

    let runner = CliRunner::new().await;

    // The CLI needs an admin caller; create one first.
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

    let doc_root = runner
        .workdir
        .join("public_html")
        .to_string_lossy()
        .into_owned();
    let domain = "cli-site.example.com";

    let create = runner.run(&[
        "site",
        "create",
        "--domain",
        domain,
        "--owner",
        "admin",
        "--document-root",
        &doc_root,
    ]);
    assert_eq!(create.code, 0, "site create failed: {}", create.stderr);
    assert!(
        create.stdout.contains(&format!("created site {domain} (")),
        "create output should confirm, got: {}",
        create.stdout
    );

    let list = runner.run(&["site", "list"]);
    assert_eq!(list.code, 0, "site list failed: {}", list.stderr);
    assert!(
        list.stdout.contains(domain),
        "list should contain the new site, got: {}",
        list.stdout
    );
}

#[tokio::test]
async fn cli_site_transport_show_then_set_http3() {
    use openpanel_domain::SiteRepository;

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
    let owner_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin'")
        .fetch_one(&runner.db.pool())
        .await
        .expect("owner id");
    let site_id = uuid::Uuid::new_v4();
    let site = openpanel_domain::Site::new(
        site_id,
        uuid::Uuid::parse_str(&owner_id).expect("uuid"),
        "transport.example.test",
        vec![],
        "/var/www/transport.example.test/public_html",
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

    // Show prints the default profile.
    let show = runner.run(&["site", "transport", "--site", &site_id, "show"]);
    assert_eq!(show.code, 0, "{}", show.stderr);
    let parsed: serde_json::Value = serde_json::from_str(show.stdout.trim()).expect("json");
    assert_eq!(parsed["http3_enabled"], serde_json::json!(false));
    assert_eq!(parsed["body_size_cap_bytes"], serde_json::json!(104857600));

    // Enable HTTP/3 and observe the flip.
    let on = runner.run(&[
        "site",
        "transport",
        "--site",
        &site_id,
        "http3",
        "--on",
        "true",
    ]);
    assert_eq!(on.code, 0, "{}", on.stderr);
    assert!(on.stdout.contains("http3 enabled"));
    let show = runner.run(&["site", "transport", "--site", &site_id, "show"]);
    let parsed: serde_json::Value = serde_json::from_str(show.stdout.trim()).expect("json");
    assert_eq!(parsed["http3_enabled"], serde_json::json!(true));

    // Disable again.
    let off = runner.run(&[
        "site",
        "transport",
        "--site",
        &site_id,
        "http3",
        "--on=false",
    ]);
    assert_eq!(off.code, 0, "{}", off.stderr);
    let show = runner.run(&["site", "transport", "--site", &site_id, "show"]);
    let parsed: serde_json::Value = serde_json::from_str(show.stdout.trim()).expect("json");
    assert_eq!(parsed["http3_enabled"], serde_json::json!(false));

    // Unknown sites are refused.
    let unknown = runner.run(&[
        "site",
        "transport",
        "--site",
        &uuid::Uuid::new_v4().to_string(),
        "show",
    ]);
    assert_ne!(unknown.code, 0);
}
