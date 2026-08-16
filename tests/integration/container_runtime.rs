//! Container runtime integration tests: per-user quota enforced at the
//! docker create surface, registry-credential lifecycle, metrics, egress
//! limits, and the /container/quota and /registry/credentials web pages.

use openpanel_test_support::TestServer;
use uuid::Uuid;

fn csrf(body: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = body.find(marker).expect("csrf") + marker.len();
    body[start..].split('"').next().expect("token").to_owned()
}

fn spec_json(id: Uuid, name: &str) -> serde_json::Value {
    serde_json::json!({"id":id,"name":name,"image":"library/redis:7","site_id":null,"env":{},"ports":[],"mounts":[],"capabilities":[],"limits":{"cpu_shares":1024,"mem_mb":64,"pids_max":64},"restart_policy":"no","user_namespace":"1001:1001"})
}

async fn owner(server: &TestServer, username: &str) -> String {
    server
        .identity()
        .create_user(
            username,
            &format!("{username}@example.test"),
            "correct horse battery staple",
            openpanel_domain::Role::Owner,
            "test",
        )
        .await
        .expect("owner");
    server.login(username, "correct horse battery staple").await
}

async fn allow_redis(server: &TestServer, token: &str) {
    let base = format!("{}/api/v1/docker", server.base_url());
    let response = server
        .client()
        .post(format!("{base}/images/allowlist"))
        .bearer_auth(token)
        .json(&serde_json::json!({"pattern":"library/redis:*","allow_pull":true,"pin_digest_required":false}))
        .send()
        .await
        .expect("allow");
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn quota_gate_blocks_a_create_that_would_exceed_max_total() {
    let server = TestServer::new().await;
    let token = owner(&server, "owner").await;
    allow_redis(&server, &token).await;
    let base = format!("{}/api/v1/docker", server.base_url());
    let quota = format!("{}/api/v1/container/quota", server.base_url());

    let set = server
        .client()
        .put(&quota)
        .bearer_auth(&token)
        .json(&serde_json::json!({"max_concurrent": 2, "max_total": 2}))
        .send()
        .await
        .expect("set quota");
    assert_eq!(set.status(), 200);

    for n in 1..=2 {
        let created = server
            .client()
            .post(format!("{base}/containers"))
            .bearer_auth(&token)
            .json(&spec_json(Uuid::new_v4(), &format!("one{n}")))
            .send()
            .await
            .expect("create");
        assert_eq!(created.status(), 200, "create {n}");
    }

    let blocked = server
        .client()
        .post(format!("{base}/containers"))
        .bearer_auth(&token)
        .json(&spec_json(Uuid::new_v4(), "three"))
        .send()
        .await
        .expect("blocked create");
    assert_eq!(blocked.status(), 422);
    assert!(
        blocked
            .text()
            .await
            .expect("body")
            .contains("validation_failed")
    );

    let events = server.audit_events().await;
    assert!(
        events
            .iter()
            .any(|event| event.action.as_str() == "container_start_quota_blocked"),
        "expected ContainerStartQuotaBlocked audit, got {events:?}"
    );
}

#[tokio::test]
async fn quota_gate_blocks_a_create_that_would_exceed_max_concurrent() {
    let server = TestServer::new().await;
    let token = owner(&server, "owner").await;
    allow_redis(&server, &token).await;
    let base = format!("{}/api/v1/docker", server.base_url());
    let quota = format!("{}/api/v1/container/quota", server.base_url());

    let set = server
        .client()
        .put(&quota)
        .bearer_auth(&token)
        .json(&serde_json::json!({"max_concurrent": 1, "max_total": 10}))
        .send()
        .await
        .expect("set quota");
    assert_eq!(set.status(), 200);

    let first = server
        .client()
        .post(format!("{base}/containers"))
        .bearer_auth(&token)
        .json(&spec_json(Uuid::new_v4(), "first"))
        .send()
        .await
        .expect("create");
    assert_eq!(first.status(), 200);
    let first_id = first.json::<serde_json::Value>().await.expect("json")["spec"]["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let started = server
        .client()
        .post(format!("{base}/containers/{first_id}/start"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("start");
    assert_eq!(started.status(), 200);

    let blocked = server
        .client()
        .post(format!("{base}/containers"))
        .bearer_auth(&token)
        .json(&spec_json(Uuid::new_v4(), "second"))
        .send()
        .await
        .expect("blocked create");
    assert_eq!(blocked.status(), 422);
}

#[tokio::test]
async fn registry_credential_lifecycle_is_encrypted_and_redacted() {
    let server = TestServer::new().await;
    let token = owner(&server, "owner").await;
    let base = format!("{}/api/v1/container", server.base_url());
    let creds = format!("{base}/registry/credentials");

    let created = server
        .client()
        .post(&creds)
        .bearer_auth(&token)
        .json(&serde_json::json!({"registry":"registry.example.com","username":"robot","password":"aB1!very-long-password"}))
        .send()
        .await
        .expect("create credential");
    assert_eq!(created.status(), 201);
    let body: serde_json::Value = created.json().await.expect("json");
    assert!(body["plaintext_once"].is_string(), "{body}");
    assert_eq!(body["credential"]["username"], "robot");

    let listed = server
        .client()
        .get(&creds)
        .bearer_auth(&token)
        .send()
        .await
        .expect("list");
    assert_eq!(listed.status(), 200);
    let listed_body = listed.text().await.expect("list body");
    assert!(
        !listed_body.contains("aB1!very-long-password"),
        "plaintext must never be echoed on read"
    );

    let id = body["credential"]["id"].as_str().expect("credential id");
    let removed = server
        .client()
        .delete(format!("{creds}/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("remove");
    assert_eq!(removed.status(), 200);
}

#[tokio::test]
async fn metrics_and_egress_limits_are_exposed_over_the_api() {
    let server = TestServer::new().await;
    let token = owner(&server, "owner").await;
    let base = format!("{}/api/v1/container", server.base_url());
    let id = Uuid::new_v4();

    let recorded = server
        .client()
        .post(format!("{base}/containers/{id}/metrics"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"cpu_pct":12,"memory_bytes":1048576,"net_rx":100,"net_tx":200,"exits":0}))
        .send()
        .await
        .expect("record metrics");
    assert_eq!(recorded.status(), 201);

    let metrics = server
        .client()
        .get(format!("{base}/containers/{id}/metrics"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list metrics");
    assert_eq!(metrics.status(), 200);
    let body = metrics.text().await.expect("metrics body");
    assert!(body.contains("cpu_pct"), "{body}");

    let raised = server
        .client()
        .put(format!("{base}/containers/{id}/egress/limit"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"bytes_per_month": 2_000_000_000}))
        .send()
        .await
        .expect("raise egress");
    assert_eq!(raised.status(), 200);

    let events = server.audit_events().await;
    assert!(
        events
            .iter()
            .any(|event| event.action.as_str() == "container_egress_limit_raised"),
        "expected ContainerEgressLimitRaised audit"
    );
}

#[tokio::test]
async fn pull_image_resolves_a_stored_credential_and_audits() {
    let server = TestServer::new().await;
    let token = owner(&server, "owner").await;
    let base = format!("{}/api/v1/container", server.base_url());
    let container_id = Uuid::new_v4();

    let created = server
        .client()
        .post(format!("{base}/registry/credentials"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"registry":"registry.example.com","username":"robot","password":"aB1!very-long-password"}))
        .send()
        .await
        .expect("create credential");
    assert_eq!(created.status(), 201);
    let body: serde_json::Value = created.json().await.expect("json");
    let credential_id = body["credential"]["id"].as_str().expect("id");

    let pulled = server
        .client()
        .post(format!("{base}/containers/{container_id}/pull"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"image_ref":"registry.example.com/app:v1","registry_credential_id":credential_id}))
        .send()
        .await
        .expect("pull");
    assert_eq!(pulled.status(), 200);

    let events = server.audit_events().await;
    assert!(
        events
            .iter()
            .any(|event| event.action.as_str() == "container_image_pulled"),
        "expected ContainerImagePulled audit"
    );
}

#[tokio::test]
async fn web_pages_require_owner_and_csrf_on_mutation() {
    let server = TestServer::new().await;
    server
        .identity()
        .create_user(
            "owner",
            "owner@example.test",
            "correct horse battery staple",
            openpanel_domain::Role::Owner,
            "test",
        )
        .await
        .expect("owner");
    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "owner"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .expect("login");
    let cookie = login.headers()[reqwest::header::SET_COOKIE]
        .to_str()
        .expect("cookie")
        .split(';')
        .next()
        .expect("pair")
        .to_owned();

    let quota_page = server
        .client()
        .get(format!("{}/container/quota", server.base_url()))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("quota page");
    assert_eq!(quota_page.status(), 200);
    let quota_body = quota_page.text().await.expect("quota html");
    assert!(quota_body.contains("Container quota"));
    let quota_csrf = csrf(&quota_body);

    let updated = server
        .client()
        .post(format!("{}/container/quota", server.base_url()))
        .header("cookie", &cookie)
        .form(&[
            ("_csrf", quota_csrf.as_str()),
            ("max_concurrent", "5"),
            ("max_total", "25"),
        ])
        .send()
        .await
        .expect("quota update");
    assert_eq!(updated.status(), 200);

    let bad = server
        .client()
        .post(format!("{}/container/quota", server.base_url()))
        .header("cookie", &cookie)
        .form(&[("_csrf", "wrong"), ("max_concurrent", "5")])
        .send()
        .await
        .expect("bad csrf");
    assert_eq!(bad.status(), 403);

    let registry_page = server
        .client()
        .get(format!("{}/registry/credentials", server.base_url()))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("registry page");
    assert_eq!(registry_page.status(), 200);
    let registry_body = registry_page.text().await.expect("registry html");
    assert!(registry_body.contains("Registry credentials"));
    let registry_csrf = csrf(&registry_body);

    let added = server
        .client()
        .post(format!("{}/registry/credentials", server.base_url()))
        .header("cookie", &cookie)
        .form(&[
            ("_csrf", registry_csrf.as_str()),
            ("registry", "registry.example.com"),
            ("username", "robot"),
            ("password", "aB1!very-long-password"),
        ])
        .send()
        .await
        .expect("add credential");
    assert_eq!(added.status(), 200);
    assert!(
        !added
            .text()
            .await
            .expect("html")
            .contains("aB1!very-long-password"),
        "web pages must never render the plaintext password"
    );
}
