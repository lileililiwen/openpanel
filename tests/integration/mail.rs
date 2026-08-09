use crate::common::*;
fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn mail_routes_require_authentication() {
    let server = TestServer::new().await;
    for path in [
        "/api/v1/mail/readiness",
        "/api/v1/mail/domains",
        "/api/v1/mail/status",
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
async fn owner_mail_domain_mailbox_alias_quota_password_and_status_workflow() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    assert_eq!(
        server
            .client()
            .get(format!("{}/api/v1/mail/readiness", server.base_url()))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let domain = server
        .client()
        .post(format!("{}/api/v1/mail/domains", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({"name":"example.test"}))
        .send()
        .await
        .unwrap();
    assert_eq!(domain.status(), 201);
    let domain: serde_json::Value = domain.json().await.unwrap();
    let id = domain["id"].as_str().unwrap();
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/api/v1/mail/domains/{id}/enable",
                server.base_url()
            ))
            .header("authorization", &auth)
            .json(&serde_json::json!({"acknowledged":false}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/api/v1/mail/domains/{id}/disable",
                server.base_url()
            ))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let mailbox=server.client().post(format!("{}/api/v1/mail/domains/{id}/mailboxes",server.base_url())).header("authorization",&auth).json(&serde_json::json!({"local":"alice","quota_bytes":1048576,"password":"mailbox-strong-password"})).send().await.unwrap();
    assert_eq!(mailbox.status(), 201);
    let text = mailbox.text().await.unwrap();
    assert!(text.contains("mailbox-strong-password"));
    let list = server
        .client()
        .get(format!(
            "{}/api/v1/mail/domains/{id}/mailboxes",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!list.contains("mailbox-strong-password"));
    assert!(!list.contains("password_hash"));
    assert_eq!(
        server
            .client()
            .get(format!("{}/api/v1/mail/status", server.base_url()))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}

#[tokio::test]
async fn mail_web_requires_csrf() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
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
    assert_eq!(
        server
            .client()
            .get(format!("{}/mail", server.base_url()))
            .header("cookie", cookie)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        server
            .client()
            .post(format!("{}/mail/domains", server.base_url()))
            .header("cookie", cookie)
            .form(&[("_csrf", "wrong"), ("name", "example.test")])
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
}

#[tokio::test]
async fn domain_delete_reports_dependencies_and_consumes_confirmation() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let created = server
        .client()
        .post(format!("{}/api/v1/mail/domains", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({"name":"delete.test"}))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    server
        .client()
        .post(format!(
            "{}/api/v1/mail/domains/{id}/mailboxes",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({"local":"alice","quota_bytes":1048576,"password":"mailbox-strong-password"}))
        .send()
        .await
        .unwrap();
    let preview = server
        .client()
        .post(format!(
            "{}/api/v1/mail/domains/{id}/delete-preview",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), 200);
    let preview = preview.json::<serde_json::Value>().await.unwrap();
    assert_eq!(preview["mailboxes"], 1);
    let confirmation = preview["confirmation_token"].as_str().unwrap();
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/api/v1/mail/domains/{id}/delete",
                server.base_url()
            ))
            .header("authorization", &auth)
            .json(&serde_json::json!({"confirmation_token":"wrong"}))
            .send()
            .await
            .unwrap()
            .status(),
        422
    );
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/api/v1/mail/domains/{id}/delete",
                server.base_url()
            ))
            .header("authorization", &auth)
            .json(&serde_json::json!({"confirmation_token":confirmation}))
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
}
