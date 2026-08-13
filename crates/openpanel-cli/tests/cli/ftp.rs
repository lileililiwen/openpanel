//! FTP CLI lifecycle against an isolated SQLite database.

use super::common::CliRunner;

#[tokio::test]
async fn cli_ftp_create_list_disable_enable_and_delete() {
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
    let site_id = uuid::Uuid::new_v4();
    let _ = runner.run(&["ftp", "list", "--site", &site_id.to_string()]);
    let owner_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username='owner'")
        .fetch_one(&runner.db.pool())
        .await
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    let home = runner.workdir.join("public");
    std::fs::create_dir_all(&home).unwrap();
    sqlx::query("INSERT INTO sites (id,owner_id,primary_domain,aliases,document_root,php_enabled,status,created_at,updated_at,created_by,modified_by) VALUES (?,?,?,?,?,0,'active',?,?,?,'')").bind(site_id.to_string()).bind(owner_id).bind("ftp-cli.example.test").bind("[]").bind(home.to_string_lossy().as_ref()).bind(&now).bind(&now).bind("owner").execute(&runner.db.pool()).await.unwrap();
    let site = site_id.to_string();
    let created = runner.run(&[
        "ftp",
        "create",
        "--site",
        &site,
        "--username",
        "uploads",
        "--password",
        "correct horse battery staple",
    ]);
    assert_eq!(created.code, 0, "{}", created.stderr);
    assert!(!created.stdout.contains("argon2"));
    let value: serde_json::Value = serde_json::from_str(&created.stdout).unwrap();
    let id = value["id"].as_str().unwrap();
    let listed = runner.run(&["ftp", "list", "--site", &site]);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert!(listed.stdout.contains("uploads"));
    assert!(!listed.stdout.contains("password_hash"));
    for action in ["disable", "enable"] {
        let result = runner.run(&["ftp", action, "--site", &site, "--id", id]);
        assert_eq!(result.code, 0, "{action}: {}", result.stderr);
    }
    let deleted = runner.run(&["ftp", "delete", "--site", &site, "--id", id]);
    assert_eq!(deleted.code, 0, "{}", deleted.stderr);
}
