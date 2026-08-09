use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn system_service_routes_require_authentication() {
    let server = TestServer::new().await;
    for path in [
        "/api/v1/services",
        "/api/v1/services/nginx",
        "/api/v1/services/nginx/history",
        "/api/v1/services/nginx/logs",
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
async fn owner_previews_and_controls_only_registered_services() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let list = server
        .client()
        .get(format!("{}/api/v1/services", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let body: serde_json::Value = list.json().await.unwrap();
    assert!(
        body.as_array()
            .unwrap()
            .iter()
            .any(|service| service["id"] == "nginx")
    );
    assert_eq!(
        server
            .client()
            .get(format!(
                "{}/api/v1/services/arbitrary.service",
                server.base_url()
            ))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let preview = server
        .client()
        .post(format!(
            "{}/api/v1/services/nginx/preview",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({"action":"restart"}))
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), 200);
    assert!(preview.text().await.unwrap().contains("sites"));
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/api/v1/services/nginx/actions",
                server.base_url()
            ))
            .header("authorization", &auth)
            .json(&serde_json::json!({"action":"restart","confirmed":true}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}

#[tokio::test]
async fn service_web_mutation_requires_owner_confirmation_and_csrf() {
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
    let page = server
        .client()
        .get(format!("{}/services", server.base_url()))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.text().await.unwrap().contains("System services"));
    assert_eq!(
        server
            .client()
            .post(format!("{}/services/nginx/actions", server.base_url()))
            .header("cookie", cookie)
            .form(&[
                ("_csrf", "wrong"),
                ("action", "restart"),
                ("confirmed", "true")
            ])
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
}
