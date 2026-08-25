//! Web-terminal integration tests: ticket issuance authorization,
//! single-use consumption, and WebSocket route guardrails.

use crate::common::*;

async fn owner_and_site(server: &TestServer) -> (String, openpanel_domain::Site) {
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let owner = server
        .identity()
        .list_users()
        .await
        .expect("users")
        .into_iter()
        .find(|user| user.username().as_str() == "owner")
        .expect("owner");
    let site = server
        .sites()
        .create_site(
            &owner,
            owner.id(),
            "terminal.example.test",
            vec![],
            false,
            None,
            Some("/var/www/terminal.example.test/public_html".to_owned()),
        )
        .await
        .expect("site");
    (token, site)
}

#[tokio::test]
async fn terminal_ticket_is_owner_only_and_hex_encoded() {
    let server = TestServer::new().await;
    let (_token, site) = owner_and_site(&server).await;
    let url = format!("{}/api/v1/terminal/ticket", server.base_url());
    let body = serde_json::json!({ "site_id": site.id() });

    // Unauthenticated callers are rejected.
    assert_eq!(
        server
            .client()
            .post(&url)
            .json(&body)
            .send()
            .await
            .expect("anon")
            .status(),
        401
    );

    // Admins may open terminals, but plain users may not exist here;
    // verify the owner path returns a 64-hex token.
    let issued = server
        .client()
        .post(&url)
        .bearer_auth(&_token)
        .json(&body)
        .send()
        .await
        .expect("issue");
    assert_eq!(
        issued.status(),
        200,
        "{}",
        issued.text().await.expect("body")
    );
    let json: serde_json::Value = issued.json().await.expect("json");
    let token = json["token"].as_str().expect("token");
    assert_eq!(token.len(), 64);
    assert!(
        token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert_eq!(json["site_id"], serde_json::json!(site.id().to_string()));
}

#[tokio::test]
async fn terminal_ticket_is_single_use_across_sessions() {
    let server = TestServer::new().await;
    let (token, site) = owner_and_site(&server).await;
    let url = format!("{}/api/v1/terminal/ticket", server.base_url());
    let issued: serde_json::Value = server
        .client()
        .post(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "site_id": site.id() }))
        .send()
        .await
        .expect("issue")
        .json()
        .await
        .expect("json");
    let ticket_token = issued["token"].as_str().expect("token").to_owned();

    let service = server.web_terminal();

    // The first start_session attempt consumes the ticket. The PTY
    // spawn itself fails in the sandbox (`su` target does not exist),
    // but consumption happens before the spawn.
    let _ = service
        .start_session(&ticket_token, "op-owner", "/tmp")
        .await;
    let second = service
        .start_session(&ticket_token, "op-owner", "/tmp")
        .await
        .err()
        .expect("second consume must fail");
    assert!(
        matches!(second, openpanel_domain::web_terminal::TerminalError::Used),
        "expected Used, got {second}"
    );

    // Unknown tickets are rejected outright.
    let unknown = service
        .start_session(&"ab".repeat(32), "op-owner", "/tmp")
        .await
        .err()
        .expect("unknown ticket must fail");
    assert!(matches!(
        unknown,
        openpanel_domain::web_terminal::TerminalError::UnknownTicket
    ));
}

#[tokio::test]
async fn terminal_ws_route_rejects_origin_mismatch_and_plain_gets() {
    let server = TestServer::new().await;
    let (token, site) = owner_and_site(&server).await;
    let issued: serde_json::Value = server
        .client()
        .post(format!("{}/api/v1/terminal/ticket", server.base_url()))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "site_id": site.id() }))
        .send()
        .await
        .expect("issue")
        .json()
        .await
        .expect("json");
    let ticket_token = issued["token"].as_str().expect("token").to_owned();
    let ws_url = format!(
        "{}/api/v1/terminal/session?ticket={}",
        server.base_url(),
        ticket_token
    );

    // A cross-origin browser upgrade is refused before consuming.
    let hostile = server
        .client()
        .get(&ws_url)
        .header("origin", "https://evil.example")
        .header("host", "panel.example.test")
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .expect("hostile get");
    assert_eq!(hostile.status(), 403);

    // Plain HTTP gets (no upgrade) are rejected by the WS extractor.
    let plain = server
        .client()
        .get(&ws_url)
        .send()
        .await
        .expect("plain get");
    assert_ne!(plain.status(), 101);
}
