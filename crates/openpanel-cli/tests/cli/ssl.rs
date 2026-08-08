//! CLI E2E tests for the `openpanel ssl` subcommand group.
//!
//! Only covers the offline flows: `list` (empty) and `self-signed`
//! (generates a cert locally). `issue` (ACME HTTP-01) and `upload`
//! (PEM file IO) are network / IO-heavy and live in the integration
//! suite instead.

use base64::Engine;

use crate::common::CliRunner;

/// Build a deterministic base64-encoded 32-byte master key so the
/// `ssl` module's encryption envelope can be exercised in tests.
fn test_master_key() -> String {
    base64::engine::general_purpose::STANDARD.encode([0u8; 32])
}

#[tokio::test]
async fn ssl_list_empty_for_fresh_db() {
    let runner = CliRunner::new().await;
    let key = test_master_key();
    let (cert_dir, key_dir) = sandbox_paths(&runner).await;
    let out = runner.run_with_env(
        &["ssl", "list"],
        &[
            ("OPENPANEL__DATABASE__MASTER_KEY", key.as_str()),
            (
                "OPENPANEL__SSL__PATHS__CERT_DIR",
                cert_dir.to_str().unwrap(),
            ),
            ("OPENPANEL__SSL__PATHS__KEY_DIR", key_dir.to_str().unwrap()),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("no certificates"));
}

#[tokio::test]
async fn ssl_self_signed_creates_lists_and_revokes() {
    let runner = CliRunner::new().await;
    let key = test_master_key();
    let (cert_dir, key_dir) = sandbox_paths(&runner).await;
    let env: &[(&str, &str)] = &[
        ("OPENPANEL__DATABASE__MASTER_KEY", key.as_str()),
        (
            "OPENPANEL__SSL__PATHS__CERT_DIR",
            cert_dir.to_str().unwrap(),
        ),
        ("OPENPANEL__SSL__PATHS__KEY_DIR", key_dir.to_str().unwrap()),
    ];

    let out = runner.run_with_env(&["ssl", "self-signed", "cli-ssl.example.com"], env);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("cli-ssl.example.com"),
        "stdout was: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("self_signed"),
        "stdout was: {}",
        out.stdout
    );

    let list_out = runner.run_with_env(&["ssl", "list"], env);
    assert_eq!(list_out.code, 0);
    assert!(
        list_out.stdout.contains("cli-ssl.example.com"),
        "stdout was: {}",
        list_out.stdout
    );

    let revoke_out = runner.run_with_env(&["ssl", "revoke", "cli-ssl.example.com"], env);
    assert_eq!(revoke_out.code, 0, "stderr: {}", revoke_out.stderr);

    let list_after = runner.run_with_env(&["ssl", "list"], env);
    assert_eq!(list_after.code, 0);
    assert!(
        list_after.stdout.contains("no certificates"),
        "stdout was: {}",
        list_after.stdout
    );
}

/// Build a per-test cert/key directory inside the runner's tempdir so
/// the CLI can write there without tripping permission checks on
/// `/etc/openpanel/`. Both directories auto-clean on Drop.
async fn sandbox_paths(runner: &CliRunner) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = runner.workdir.join("ssl");
    let cert_dir = root.join("certs");
    let key_dir = root.join("keys");
    tokio::fs::create_dir_all(&cert_dir).await.unwrap();
    tokio::fs::create_dir_all(&key_dir).await.unwrap();
    (cert_dir, key_dir)
}
