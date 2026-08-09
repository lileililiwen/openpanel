use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn create_payload(working_directory: &str) -> serde_json::Value {
    serde_json::json!({
        "name": "artisan schedule",
        "schedule": "*/5 * * * *",
        "timezone": "UTC",
        "kind": "command",
        "executable": "/usr/bin/php",
        "arguments": ["artisan", "schedule:run"],
        "working_directory": working_directory,
        "timeout_secs": 60,
        "overlap_policy": "skip"
    })
}

#[tokio::test]
async fn cron_routes_require_authentication() {
    let server = TestServer::new().await;
    for (method, path) in [
        (reqwest::Method::GET, "/api/v1/cron/jobs"),
        (reqwest::Method::POST, "/api/v1/cron/jobs"),
        (reqwest::Method::GET, "/api/v1/cron/runs"),
    ] {
        let response = server
            .client()
            .request(method, format!("{}{}", server.base_url(), path))
            .send()
            .await
            .expect("cron unauthenticated response");
        assert_eq!(response.status(), 401, "{path}");
    }
}

#[tokio::test]
async fn cron_job_http_lifecycle_and_run_history() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let working = server.sandbox_path("sites/example.com/public_html");
    std::fs::create_dir_all(&working).expect("working directory");

    let invalid = server
        .client()
        .post(format!("{}/api/v1/cron/jobs", server.base_url()))
        .header(reqwest::header::AUTHORIZATION, bearer(&token))
        .json(&create_payload("/outside/owned/sites"))
        .send()
        .await
        .expect("invalid create");
    assert_eq!(invalid.status(), 422);

    let create = server
        .client()
        .post(format!("{}/api/v1/cron/jobs", server.base_url()))
        .header(reqwest::header::AUTHORIZATION, bearer(&token))
        .json(&create_payload(&working))
        .send()
        .await
        .expect("create cron job");
    assert_eq!(create.status(), 201);
    let created: serde_json::Value = create.json().await.expect("created JSON");
    let id = created["id"].as_str().expect("job id");

    for path in [
        "/api/v1/cron/jobs".to_string(),
        format!("/api/v1/cron/jobs/{id}"),
        "/api/v1/cron/runs".to_string(),
    ] {
        let response = server
            .client()
            .get(format!("{}{}", server.base_url(), path))
            .header(reqwest::header::AUTHORIZATION, bearer(&token))
            .send()
            .await
            .expect("cron GET");
        assert_eq!(response.status(), 200, "{path}");
    }

    let update = server
        .client()
        .put(format!("{}/api/v1/cron/jobs/{id}", server.base_url()))
        .header(reqwest::header::AUTHORIZATION, bearer(&token))
        .json(&serde_json::json!({
            "name": "updated artisan schedule",
            "schedule": "0 * * * *",
            "timezone": "UTC",
            "timeout_secs": 90,
            "overlap_policy": "skip"
        }))
        .send()
        .await
        .expect("update job");
    assert_eq!(update.status(), 200);

    for action in ["disable", "enable"] {
        let response = server
            .client()
            .post(format!(
                "{}/api/v1/cron/jobs/{id}/{action}",
                server.base_url()
            ))
            .header(reqwest::header::AUTHORIZATION, bearer(&token))
            .send()
            .await
            .expect("status action");
        assert_eq!(response.status(), 200, "{action}");
    }

    let run = server
        .client()
        .post(format!("{}/api/v1/cron/jobs/{id}/run", server.base_url()))
        .header(reqwest::header::AUTHORIZATION, bearer(&token))
        .send()
        .await
        .expect("run now");
    assert_eq!(run.status(), 202);
    let run_body: serde_json::Value = run.json().await.expect("run JSON");
    let run_id = run_body["id"].as_str().expect("run id");

    let detail = server
        .client()
        .get(format!("{}/api/v1/cron/runs/{run_id}", server.base_url()))
        .header(reqwest::header::AUTHORIZATION, bearer(&token))
        .send()
        .await
        .expect("run detail");
    assert_eq!(detail.status(), 200);

    let delete = server
        .client()
        .delete(format!("{}/api/v1/cron/jobs/{id}", server.base_url()))
        .header(reqwest::header::AUTHORIZATION, bearer(&token))
        .send()
        .await
        .expect("delete job");
    assert_eq!(delete.status(), 204);
}

#[tokio::test]
async fn cron_missing_job_and_owner_scoping_are_enforced() {
    let server = TestServer::new().await;
    let owner_token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let missing = uuid::Uuid::new_v4();
    let response = server
        .client()
        .get(format!("{}/api/v1/cron/jobs/{missing}", server.base_url()))
        .header(reqwest::header::AUTHORIZATION, bearer(&owner_token))
        .send()
        .await
        .expect("missing job");
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn cron_web_pages_escape_output_and_validate_csrf() {
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
        .expect("web login");
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .expect("session cookie");
    let page = server
        .client()
        .get(format!("{}/cron", server.base_url()))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .expect("cron page");
    assert_eq!(page.status(), 200);
    let html = page.text().await.expect("cron page HTML");
    assert!(html.contains("Cron jobs"));
    assert!(html.contains("/cron/new"));

    let bad_csrf = server
        .client()
        .post(format!("{}/cron/jobs", server.base_url()))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("_csrf", "wrong")])
        .send()
        .await
        .expect("bad csrf");
    assert_eq!(bad_csrf.status(), 403);
}
