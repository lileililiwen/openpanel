//! E2E test: `openpanel file` write/read/list roundtrip.
//!
//! The site is seeded directly in the DB with a document_root pointing
//! at the test workdir, so this test has no external dependencies.

use super::common::CliRunner;

fn extract_user_id(stdout: &str) -> Option<&str> {
    let open = stdout.rfind('(')?;
    let close = stdout.rfind(')')?;
    if close > open {
        Some(&stdout[open + 1..close])
    } else {
        None
    }
}

#[tokio::test]
async fn cli_file_write_read_list_roundtrip() {
    let runner = CliRunner::new().await;

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
    let owner_id = extract_user_id(&user.stdout).expect("parse user id from output");

    let site_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();

    // Seed a site whose document_root is the (existing) test workdir.
    sqlx::query(
        "INSERT INTO sites (id, owner_id, primary_domain, aliases, document_root, \
         php_enabled, php_version, status, created_at, updated_at, created_by, modified_by) \
         VALUES (?, ?, ?, ?, ?, 0, NULL, 'active', ?, ?, 'admin', '')",
    )
    .bind(site_id.to_string())
    .bind(owner_id)
    .bind("cli-file.example.com")
    .bind("[]")
    .bind(runner.workdir.to_string_lossy().into_owned())
    .bind(&now)
    .bind(&now)
    .execute(&runner.db.pool())
    .await
    .expect("seed site");

    let write = runner.run(&[
        "file",
        "write",
        "--site",
        &site_id.to_string(),
        "--path",
        "hello.txt",
        "--content",
        "hello world",
    ]);
    assert_eq!(write.code, 0, "file write failed: {}", write.stderr);
    assert!(
        write.stdout.contains("wrote 11 bytes"),
        "write output should confirm, got: {}",
        write.stdout
    );

    let read = runner.run(&[
        "file",
        "read",
        "--site",
        &site_id.to_string(),
        "--path",
        "hello.txt",
    ]);
    assert_eq!(read.code, 0, "file read failed: {}", read.stderr);
    assert_eq!(read.stdout, "hello world", "read should round-trip content");

    let list = runner.run(&["file", "list", "--site", &site_id.to_string()]);
    assert_eq!(list.code, 0, "file list failed: {}", list.stderr);
    assert!(
        list.stdout.contains("hello.txt"),
        "list should contain the file, got: {}",
        list.stdout
    );
}
