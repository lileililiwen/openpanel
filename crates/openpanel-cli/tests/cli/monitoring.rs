//! CLI E2E tests for the `openpanel monitoring` subcommand group.
//!
//! Covers the offline flows only: `overview` (collects a fresh host
//! snapshot) and `history` (reads the time series, empty on a fresh
//! DB). Both run against a sandboxed SQLite DB, no network needed.

use crate::common::CliRunner;

#[tokio::test]
async fn monitoring_overview_prints_host_values() {
    let runner = CliRunner::new().await;
    let out = runner.run(&["monitoring", "overview"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("timestamp="),
        "overview must print a timestamp, stdout was: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("cpu="),
        "overview must print cpu %, stdout was: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("memory="),
        "overview must print memory %, stdout was: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("load="),
        "overview must print load average, stdout was: {}",
        out.stdout
    );
}

#[tokio::test]
async fn monitoring_history_empty_on_fresh_db() {
    let runner = CliRunner::new().await;
    let out = runner.run(&["monitoring", "history", "--metric", "Disk"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("no samples"),
        "history on a fresh DB must report no samples, stdout was: {}",
        out.stdout
    );
}

#[tokio::test]
async fn monitoring_history_rejects_unknown_metric() {
    let runner = CliRunner::new().await;
    let out = runner.run(&["monitoring", "history", "--metric", "Bogus"]);
    assert_ne!(out.code, 0, "unknown metric must exit non-zero");
    assert!(
        out.stderr.contains("Bogus"),
        "error should name the bad metric, stderr was: {}",
        out.stderr
    );
}
