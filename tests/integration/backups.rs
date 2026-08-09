use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}
fn plan_payload() -> serde_json::Value {
    serde_json::json!({ "name": "nightly panel", "resources": [{"kind": "panel_metadata"}], "schedule": "0 2 * * *", "timezone": "UTC", "retention_copies": 3 })
}

#[tokio::test]
async fn backup_routes_require_authentication() {
    let server = TestServer::new().await;
    for path in ["/api/v1/backups/plans", "/api/v1/backups/runs"] {
        assert_eq!(
            server
                .client()
                .get(format!("{}{}", server.base_url(), path))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
    }
}

#[tokio::test]
async fn backup_plan_run_verify_restore_lifecycle() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let invalid = server
        .client()
        .post(format!("{}/api/v1/backups/plans", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), 422);
    let created = server
        .client()
        .post(format!("{}/api/v1/backups/plans", server.base_url()))
        .header("authorization", &auth)
        .json(&plan_payload())
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let plan: serde_json::Value = created.json().await.unwrap();
    let id = plan["id"].as_str().unwrap();
    for path in [
        "/api/v1/backups/plans".to_string(),
        format!("/api/v1/backups/plans/{id}"),
        "/api/v1/backups/runs".to_string(),
    ] {
        assert_eq!(
            server
                .client()
                .get(format!("{}{}", server.base_url(), path))
                .header("authorization", &auth)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    let update = server
        .client()
        .put(format!("{}/api/v1/backups/plans/{id}", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({"name":"updated nightly", "retention_copies": 2}))
        .send()
        .await
        .unwrap();
    assert_eq!(update.status(), 200);
    for action in ["disable", "enable"] {
        assert_eq!(
            server
                .client()
                .post(format!(
                    "{}/api/v1/backups/plans/{id}/{action}",
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
    let run = server
        .client()
        .post(format!(
            "{}/api/v1/backups/plans/{id}/run",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(run.status(), 202);
    let run: serde_json::Value = run.json().await.unwrap();
    let run_id = run["id"].as_str().unwrap();
    for suffix in ["", "/verify", "/restore/preview"] {
        let method = if suffix.is_empty() {
            reqwest::Method::GET
        } else {
            reqwest::Method::POST
        };
        assert_eq!(
            server
                .client()
                .request(
                    method,
                    format!("{}/api/v1/backups/runs/{run_id}{suffix}", server.base_url())
                )
                .header("authorization", &auth)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    let restore = server
        .client()
        .post(format!(
            "{}/api/v1/backups/runs/{run_id}/restore",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(
            &serde_json::json!({"resources":[{"kind":"panel_metadata"}], "conflict_policy":"fail"}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(restore.status(), 202);
    assert_eq!(
        server
            .client()
            .delete(format!(
                "{}/api/v1/backups/runs/{run_id}",
                server.base_url()
            ))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    assert_eq!(
        server
            .client()
            .delete(format!("{}/api/v1/backups/plans/{id}", server.base_url()))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
}

#[tokio::test]
async fn backup_web_page_and_actions_enforce_csrf() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "admin"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .unwrap();
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let page = server
        .client()
        .get(format!("{}/backups", server.base_url()))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.text().await.unwrap().contains("Backups"));
    let bad = server
        .client()
        .post(format!("{}/backups/plans", server.base_url()))
        .header("cookie", cookie)
        .form(&[("_csrf", "wrong")])
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 403);
}
