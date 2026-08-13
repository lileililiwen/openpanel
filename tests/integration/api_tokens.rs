use chrono::{Duration, Utc};
use openpanel_test_support::TestServer;
use reqwest::StatusCode;
use serde_json::json;

async fn curl_status(url: &str, bearer: &str) -> u16 {
    let output = tokio::process::Command::new("curl")
        .args([
            "--silent",
            "--output",
            "/dev/null",
            "--write-out",
            "%{http_code}",
            "--header",
            &format!("Authorization: Bearer {bearer}"),
            url,
        ])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().parse().unwrap()
}

async fn create_token(
    server: &TestServer,
    session: &str,
    scopes: &[&str],
    cidrs: &[&str],
    expires_at: chrono::DateTime<Utc>,
) -> serde_json::Value {
    let response = server
        .client()
        .post(format!("{}/api/v1/identity/tokens", server.base_url()))
        .bearer_auth(session)
        .json(&json!({
            "label": "integration",
            "scopes": scopes,
            "expires_at": expires_at,
            "cidr_allowlist": cidrs,
        }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create response: {body}; scopes={scopes:?}; cidrs={cidrs:?}; expires={expires_at}"
    );
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn api_token_bearer_scope_cidr_expiry_rotation_and_revocation() {
    let server = TestServer::new().await;
    let session = server
        .bootstrap_owner("token-owner", "correct horse battery staple")
        .await;

    let created = create_token(
        &server,
        &session,
        &["sites:read"],
        &[],
        Utc::now() + Duration::days(1),
    )
    .await;
    let plaintext = created["plaintext_token"].as_str().unwrap();
    assert_eq!(created["shown_once"], true);
    assert_eq!(
        curl_status(&format!("{}/api/v1/sites", server.base_url()), plaintext).await,
        200
    );

    let response = server
        .client()
        .get(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(plaintext)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = server
        .client()
        .post(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(plaintext)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let metadata = server
        .client()
        .get(format!("{}/api/v1/identity/tokens", server.base_url()))
        .bearer_auth(&session)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let rendered = metadata.to_string();
    assert!(!rendered.contains(plaintext));
    assert!(!rendered.contains("\"hash\""));

    let cidr = create_token(
        &server,
        &session,
        &["sites:read"],
        &["10.0.0.0/8"],
        Utc::now() + Duration::days(1),
    )
    .await;
    let response = server
        .client()
        .get(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(cidr["plaintext_token"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let expiring = create_token(
        &server,
        &session,
        &["sites:read"],
        &[],
        Utc::now() + Duration::seconds(2),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(2_100)).await;
    let response = server
        .client()
        .get(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(expiring["plaintext_token"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let id = created["id"].as_str().unwrap();
    let rotated = server
        .client()
        .post(format!(
            "{}/api/v1/identity/tokens/{id}/rotate",
            server.base_url()
        ))
        .bearer_auth(&session)
        .send()
        .await
        .unwrap();
    assert_eq!(rotated.status(), StatusCode::OK);
    let response = server
        .client()
        .get(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(plaintext)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let rotated: serde_json::Value = rotated.json().await.unwrap();
    let rotated_id = rotated["id"].as_str().unwrap();
    let response = server
        .client()
        .delete(format!(
            "{}/api/v1/identity/tokens/{rotated_id}",
            server.base_url()
        ))
        .bearer_auth(&session)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = server
        .client()
        .get(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(rotated["plaintext_token"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        curl_status(
            &format!("{}/api/v1/sites", server.base_url()),
            rotated["plaintext_token"].as_str().unwrap()
        )
        .await,
        401
    );
}

#[tokio::test]
async fn api_token_rate_limit_returns_retry_after() {
    let server = TestServer::new_with_api_token_rate(1, 1).await;
    let session = server
        .bootstrap_owner("rate-owner", "correct horse battery staple")
        .await;
    let created = create_token(
        &server,
        &session,
        &["sites:read"],
        &[],
        Utc::now() + Duration::days(1),
    )
    .await;
    let plaintext = created["plaintext_token"].as_str().unwrap();
    let first = server
        .client()
        .get(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(plaintext)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let second = server
        .client()
        .get(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(plaintext)
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(second.headers().contains_key("retry-after"));
}

fn csrf(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html.find(marker).unwrap() + marker.len();
    html[start..].split('"').next().unwrap().to_owned()
}

#[tokio::test]
async fn api_token_web_surface_enforces_csrf_and_shows_plaintext_once() {
    let server = TestServer::new().await;
    let session = server
        .bootstrap_owner("web-token-owner", "correct horse battery staple")
        .await;
    let page = server
        .client()
        .get(format!("{}/settings/tokens", server.base_url()))
        .header("cookie", format!("openpanel_session={session}"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let page = page.text().await.unwrap();
    let csrf = csrf(&page);
    let denied = server
        .client()
        .post(format!("{}/settings/tokens", server.base_url()))
        .header("cookie", format!("openpanel_session={session}"))
        .form(&[
            ("_csrf", "wrong"),
            ("label", "browser"),
            ("scopes", "sites:read"),
            ("expires_in_days", "1"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let created = server
        .client()
        .post(format!("{}/settings/tokens", server.base_url()))
        .header("cookie", format!("openpanel_session={session}"))
        .form(&[
            ("_csrf", csrf.as_str()),
            ("label", "browser"),
            ("scopes", "sites:read"),
            ("expires_in_days", "1"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created_html = created.text().await.unwrap();
    assert!(created_html.contains("shown exactly once"));
    let marker = "openpanel_pat_";
    let start = created_html.find(marker).unwrap();
    let plaintext = created_html[start..].split('<').next().unwrap().to_owned();
    let refreshed = server
        .client()
        .get(format!("{}/settings/tokens", server.base_url()))
        .header("cookie", format!("openpanel_session={session}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!refreshed.contains(&plaintext));
}
