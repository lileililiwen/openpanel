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
