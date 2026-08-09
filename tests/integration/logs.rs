use std::io::Write;

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn log_routes_require_authentication() {
    let server = TestServer::new().await;
    for path in [
        "/api/v1/logs/sources",
        "/api/v1/logs/entries",
        "/api/v1/logs/traffic",
        "/api/v1/logs/audit",
        "/api/v1/logs/retention",
    ] {
        let response = server
            .client()
            .get(format!("{}{}", server.base_url(), path))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401, "{path}");
    }
}

#[tokio::test]
async fn logs_list_tail_filter_traffic_audit_and_bounded_download() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);

    let sources = server
        .client()
        .get(format!("{}/api/v1/logs/sources", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(sources.status(), 200);
    let sources: serde_json::Value = sources.json().await.unwrap();
    let source_id = sources.as_array().unwrap()[0]["id"].as_str().unwrap();

    let log_path = server.sandbox_path("logs/panel-error.log");
    std::fs::create_dir_all(std::path::Path::new(&log_path).parent().unwrap()).unwrap();
    std::fs::write(
        &log_path,
        "2026-08-09T02:00:00Z ERROR failed /login?token=secret\n2026-08-09T02:01:00Z INFO recovered\n",
    )
    .unwrap();

    let entries = server
        .client()
        .get(format!(
            "{}/api/v1/logs/entries?source_id={source_id}&text=failed&limit=10",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(entries.status(), 200);
    let body = entries.text().await.unwrap();
    assert!(body.contains("failed"));
    assert!(!body.contains("secret"));
    assert!(!body.contains(&log_path));

    for path in ["traffic", "audit"] {
        assert_eq!(
            server
                .client()
                .get(format!("{}/api/v1/logs/{path}", server.base_url()))
                .header("authorization", &auth)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }

    let download = server
        .client()
        .get(format!(
            "{}/api/v1/logs/download?source_id={source_id}",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(download.status(), 200);
    assert!(
        download.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );
    assert!(!download.text().await.unwrap().contains("secret"));
    assert!(
        server
            .audit_events()
            .await
            .iter()
            .any(|event| event.action == openpanel_core::AuditAction::LogDownloaded)
    );
}

#[tokio::test]
async fn logs_web_page_escapes_entries_and_redirects_unauthenticated_users() {
    let server = TestServer::new().await;
    assert_eq!(
        server
            .client()
            .get(format!("{}/logs", server.base_url()))
            .send()
            .await
            .unwrap()
            .status(),
        302
    );
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
    let cookie = login.headers()[reqwest::header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let page = server
        .client()
        .get(format!("{}/logs", server.base_url()))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.text().await.unwrap().contains("Logs &amp; traffic"));
    let path = server.sandbox_path("logs/panel-error.log");
    std::fs::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
    std::fs::write(&path, "<script>alert('unsafe')</script>\n").unwrap();
    let fragment = server
        .client()
        .get(format!(
            "{}/logs/entries?source_id=00000000-0000-0000-0000-000000000001",
            server.base_url()
        ))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(fragment.contains("&lt;script&gt;"));
    assert!(!fragment.contains("<script>"));
}

#[tokio::test]
async fn traffic_aggregation_is_idempotent_and_site_sources_are_registered() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let owner: serde_json::Value = server
        .client()
        .get(format!("{}/api/v1/identity/me", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let created = server
        .client()
        .post(format!("{}/api/v1/sites", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "owner_id": owner["id"],
            "primary_domain": "logs.example.test",
            "aliases": [],
            "php_enabled": false,
            "document_root": server.sandbox_path("sites/logs/public_html")
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 200);
    let site: serde_json::Value = created.json().await.unwrap();
    let site_id = site["id"].as_str().unwrap();
    let access_path = server.sandbox_path(&format!("logs/{site_id}.access.log"));
    std::fs::create_dir_all(std::path::Path::new(&access_path).parent().unwrap()).unwrap();
    std::fs::write(
        &access_path,
        format!("{site_id}\t2026-08-09T02:00:00Z\tGET\t/docs\t200\t512\t17\t192.0.2.1\tagent\n"),
    )
    .unwrap();

    assert_eq!(server.logs().aggregate_once().await.unwrap(), 1);
    assert_eq!(server.logs().aggregate_once().await.unwrap(), 0);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&access_path)
        .unwrap()
        .write_all(
            format!("{site_id}\t2026-08-09T02:01:00Z\tGET\t/docs\t404\t128\t3\t192.0.2.2\tagent\n")
                .as_bytes(),
        )
        .unwrap();
    assert_eq!(server.logs().aggregate_once().await.unwrap(), 1);

    let sources: serde_json::Value = server
        .client()
        .get(format!("{}/api/v1/logs/sources", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        sources
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["site_id"] == site_id)
    );
    let traffic: serde_json::Value = server
        .client()
        .get(format!("{}/api/v1/logs/traffic", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(traffic[0]["requests"], 2);
    assert_eq!(traffic[0]["response_bytes"], 640);
}

#[tokio::test]
async fn large_tail_is_bounded_and_rotation_resumes_on_the_active_file() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let sources: serde_json::Value = server
        .client()
        .get(format!("{}/api/v1/logs/sources", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let source_id = sources[0]["id"].as_str().unwrap();
    let path = server.sandbox_path("logs/panel-error.log");
    std::fs::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
    let mut content = "early-secret-line\n".to_string();
    content.push_str(&"bounded filler line\n".repeat(70_000));
    content.push_str("latest-visible-line\n");
    std::fs::write(&path, content).unwrap();
    let first: serde_json::Value = server
        .client()
        .get(format!(
            "{}/api/v1/logs/entries?source_id={source_id}&limit=2",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(first.to_string().contains("latest-visible-line"));
    assert!(!first.to_string().contains("early-secret-line"));
    let cursor = first["cursor"].as_str().unwrap();

    std::fs::rename(&path, format!("{path}.1")).unwrap();
    std::fs::write(&path, "new-active-file\n").unwrap();
    let second: serde_json::Value = server
        .client()
        .get(format!(
            "{}/api/v1/logs/entries?source_id={source_id}&limit=2&cursor={cursor}",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(second["rotated"], true);
    assert!(second.to_string().contains("new-active-file"));
}

#[tokio::test]
async fn user_cannot_discover_or_read_another_users_site_source() {
    let server = TestServer::new().await;
    let owner_token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let alice = server
        .identity()
        .create_user(
            "alice",
            "alice@example.test",
            "correct horse battery staple",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .unwrap();
    server
        .identity()
        .create_user(
            "bob",
            "bob@example.test",
            "correct horse battery staple",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .unwrap();
    let created: serde_json::Value = server
        .client()
        .post(format!("{}/api/v1/sites", server.base_url()))
        .header("authorization", bearer(&owner_token))
        .json(&serde_json::json!({"owner_id":alice.id(), "primary_domain":"alice.example.test", "aliases":[], "php_enabled":false, "document_root":server.sandbox_path("sites/alice/public_html")}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let site_id = created["id"].as_str().unwrap();
    let owner_sources: serde_json::Value = server
        .client()
        .get(format!("{}/api/v1/logs/sources", server.base_url()))
        .header("authorization", bearer(&owner_token))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let source_id = owner_sources
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["site_id"] == site_id && source["kind"] == "error")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let bob_token = server.login("bob", "correct horse battery staple").await;
    let response = server
        .client()
        .get(format!(
            "{}/api/v1/logs/entries?source_id={source_id}",
            server.base_url()
        ))
        .header("authorization", bearer(&bob_token))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    assert!(
        !response
            .text()
            .await
            .unwrap()
            .contains("alice.example.test")
    );
}
