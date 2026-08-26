use openpanel_domain::{Role, Session, SessionRepository, SessionToken};

use super::common::CliRunner;

#[tokio::test]
async fn cli_auth_sessions_list_and_revoke() {
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

    // Two parallel logins for the same user.
    let owner_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin'")
        .fetch_one(&runner.db.pool())
        .await
        .expect("owner id");
    let owner_uuid = uuid::Uuid::parse_str(&owner_id).expect("uuid");
    let repo = openpanel_app::identity::repo::SqliteSessionRepository::new(runner.db.pool());
    let mut session_ids = Vec::new();
    for ua in ["cli-session-one", "cli-session-two"] {
        let token = SessionToken::generate();
        let (session, _) = Session::new(
            owner_uuid,
            Role::Owner,
            &token,
            Some("127.0.0.1".into()),
            Some(ua.into()),
        );
        repo.insert(&session).await.expect("insert session");
        session_ids.push(session.id.to_string());
    }

    // Both sessions are listed.
    let listed = runner.run(&["auth", "sessions"]);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    let parsed: serde_json::Value = serde_json::from_str(listed.stdout.trim()).expect("json");
    assert_eq!(parsed.as_array().expect("sessions").len(), 2);
    assert!(
        !listed.stdout.contains("token"),
        "session tokens must never be listed"
    );

    // Revoke the first session; only the second remains.
    let revoked = runner.run(&["auth", "revoke", "--session", &session_ids[0]]);
    assert_eq!(revoked.code, 0, "{}", revoked.stderr);
    let listed = runner.run(&["auth", "sessions"]);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    let parsed: serde_json::Value = serde_json::from_str(listed.stdout.trim()).expect("json");
    let remaining = parsed.as_array().expect("sessions");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0]["id"].as_str().expect("id"), session_ids[1]);

    // Unknown session ids are refused.
    let unknown = runner.run(&[
        "auth",
        "revoke",
        "--session",
        &uuid::Uuid::new_v4().to_string(),
    ]);
    assert_ne!(unknown.code, 0);
}
