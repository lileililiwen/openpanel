use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}
fn rule_payload() -> serde_json::Value {
    serde_json::json!({"protocol":"tcp","port_start":443,"port_end":443,"source":"0.0.0.0/0","action":"allow","comment":"HTTPS","enabled":true})
}

#[tokio::test]
async fn security_routes_require_authentication() {
    let server = TestServer::new().await;
    for path in [
        "/api/v1/security/status",
        "/api/v1/security/rules",
        "/api/v1/security/blocks",
        "/api/v1/security/posture",
    ] {
        assert_eq!(
            server
                .client()
                .get(format!("{}{}", server.base_url(), path))
                .send()
                .await
                .unwrap()
                .status(),
            401,
            "{path}"
        );
    }
}

#[tokio::test]
async fn owner_firewall_rule_preview_apply_and_rollback_lifecycle() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    assert_eq!(
        server
            .client()
            .get(format!("{}/api/v1/security/status", server.base_url()))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let created = server
        .client()
        .post(format!("{}/api/v1/security/rules", server.base_url()))
        .header("authorization", &auth)
        .json(&rule_payload())
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let rule: serde_json::Value = created.json().await.unwrap();
    let id = rule["id"].as_str().unwrap();
    assert_eq!(
        server
            .client()
            .get(format!("{}/api/v1/security/rules", server.base_url()))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let preview = server
        .client()
        .post(format!("{}/api/v1/security/preview", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), 200);
    assert!(
        preview
            .text()
            .await
            .unwrap()
            .contains("table inet openpanel")
    );
    assert_eq!(
        server
            .client()
            .post(format!("{}/api/v1/security/apply", server.base_url()))
            .header("authorization", &auth)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    for action in ["disable", "enable"] {
        assert_eq!(
            server
                .client()
                .post(format!(
                    "{}/api/v1/security/rules/{id}/{action}",
                    server.base_url()
                ))
                .header("authorization", &auth)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    assert_eq!(
        server
            .client()
            .put(format!("{}/api/v1/security/rules/{id}", server.base_url()))
            .header("authorization", &auth)
            .json(&serde_json::json!({"comment":"updated HTTPS"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        server
            .client()
            .post(format!("{}/api/v1/security/rollback", server.base_url()))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        server
            .client()
            .delete(format!("{}/api/v1/security/rules/{id}", server.base_url()))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
}

#[tokio::test]
async fn admin_cannot_mutate_firewall_and_security_web_requires_owner_and_csrf() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    server
        .identity()
        .create_user(
            "operator",
            "operator@example.test",
            "correct horse battery staple",
            openpanel_domain::Role::Admin,
            "test",
        )
        .await
        .unwrap();
    let token = server
        .login("operator", "correct horse battery staple")
        .await;
    assert_eq!(
        server
            .client()
            .post(format!("{}/api/v1/security/rules", server.base_url()))
            .header("authorization", bearer(&token))
            .json(&rule_payload())
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "owner"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .unwrap();
    let cookie = login.headers()[reqwest::header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let page = server
        .client()
        .get(format!("{}/security", server.base_url()))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.text().await.unwrap().contains("Host security"));
    assert_eq!(
        server
            .client()
            .post(format!("{}/security/rules", server.base_url()))
            .header("cookie", cookie)
            .form(&[("_csrf", "wrong")])
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
}

#[tokio::test]
async fn login_abuse_is_generic_rate_limited_and_ignores_forged_forwarding() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("alice", "correct horse battery staple")
        .await;
    for _ in 0..5 {
        let response = server
            .client()
            .post(format!("{}/api/v1/identity/login", server.base_url()))
            .header("x-forwarded-for", "198.51.100.77")
            .json(&serde_json::json!({"username_or_email":"alice","password":"wrong password"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
    }
    let blocked = server.client().post(format!("{}/api/v1/identity/login", server.base_url()))
        .header("x-forwarded-for", "198.51.100.77")
        .json(&serde_json::json!({"username_or_email":"alice","password":"correct horse battery staple"}))
        .send().await.unwrap();
    assert_eq!(blocked.status(), 401);
    assert!(blocked.headers().contains_key("retry-after"));
    let blocks = server.security().blocks().await.unwrap();
    assert!(
        blocks
            .iter()
            .any(|block| block.key().storage_key() == "account:alice")
    );
    assert!(
        !blocks
            .iter()
            .any(|block| block.key().storage_key() == "ip:198.51.100.77")
    );
}

#[tokio::test]
async fn owner_manages_login_address_allowlists() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let created = server
        .client()
        .post(format!("{}/api/v1/security/allowlists", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({"network":"192.0.2.0/24"}))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let listed: serde_json::Value = server
        .client()
        .get(format!("{}/api/v1/security/allowlists", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(
        server
            .client()
            .delete(format!(
                "{}/api/v1/security/allowlists/192.0.2.0%2F24",
                server.base_url()
            ))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
}
