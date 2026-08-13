//! API-token CLI lifecycle against an isolated SQLite database.

use super::common::CliRunner;

#[tokio::test]
async fn cli_token_create_list_rotate_and_revoke_never_relists_plaintext() {
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
    let created = runner.run(&[
        "token",
        "create",
        "--label",
        "ci",
        "--scope",
        "sites:read",
        "--expires-in-days",
        "1",
    ]);
    assert_eq!(created.code, 0, "{}", created.stderr);
    let created_json: serde_json::Value = serde_json::from_str(&created.stdout).unwrap();
    let plaintext = created_json["plaintext_token"].as_str().unwrap();
    let id = created_json["id"].as_str().unwrap();
    assert!(plaintext.starts_with("openpanel_pat_"));

    let listed = runner.run(&["token", "list"]);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert!(!listed.stdout.contains(plaintext));
    assert!(!listed.stdout.contains("\"hash\""));

    let rotated = runner.run(&["token", "rotate", "--id", id]);
    assert_eq!(rotated.code, 0, "{}", rotated.stderr);
    let rotated_json: serde_json::Value = serde_json::from_str(&rotated.stdout).unwrap();
    assert_ne!(rotated_json["plaintext_token"].as_str().unwrap(), plaintext);
    let rotated_id = rotated_json["id"].as_str().unwrap();

    let revoked = runner.run(&["token", "revoke", "--id", rotated_id]);
    assert_eq!(revoked.code, 0, "{}", revoked.stderr);
}
