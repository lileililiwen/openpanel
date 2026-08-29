//! CLI E2E tests for the `openpanel status-page` subcommand group.

use crate::common::CliRunner;

#[tokio::test]
async fn cli_status_page_enable_then_show_slug() {
    let runner = CliRunner::new().await;

    let out = runner.run(&["status-page", "show"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("\"slug\""),
        "show must print a slug, stdout was: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("\"enabled\""),
        "show must print enabled flag, stdout was: {}",
        out.stdout
    );

    let enable = runner.run(&["status-page", "enable"]);
    assert_eq!(enable.code, 0, "stderr: {}", enable.stderr);
    assert!(
        enable.stdout.contains("enabled"),
        "enable must print confirmation, stdout was: {}",
        enable.stdout
    );

    let after = runner.run(&["status-page", "show"]);
    assert_eq!(after.code, 0, "stderr: {}", after.stderr);
    assert!(
        after.stdout.contains("\"enabled\": true"),
        "show after enable must report enabled=true, stdout was: {}",
        after.stdout
    );

    let disable = runner.run(&["status-page", "disable"]);
    assert_eq!(disable.code, 0, "stderr: {}", disable.stderr);

    let after = runner.run(&["status-page", "show"]);
    assert!(
        after.stdout.contains("\"enabled\": false"),
        "show after disable must report enabled=false, stdout was: {}",
        after.stdout
    );
}

#[tokio::test]
async fn cli_status_page_regenerate_slug_changes_slug() {
    let runner = CliRunner::new().await;
    let _ = runner.run(&["status-page", "enable"]);

    let before = runner.run(&["status-page", "show"]);
    assert_eq!(before.code, 0, "stderr: {}", before.stderr);

    let regen = runner.run(&["status-page", "regenerate-slug"]);
    assert_eq!(regen.code, 0, "stderr: {}", regen.stderr);

    let after = runner.run(&["status-page", "show"]);
    assert_eq!(after.code, 0, "stderr: {}", after.stderr);
    assert_ne!(
        before.stdout, after.stdout,
        "regenerate-slug should change the visible slug"
    );
}
