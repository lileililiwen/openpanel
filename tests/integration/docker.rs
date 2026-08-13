//! Container API and web surface integration tests.

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use openpanel_domain::docker::{ComposeStack, ImageAllowlistEntry};
use openpanel_test_support::TestServer;

fn csrf(body: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = body.find(marker).expect("csrf") + marker.len();
    body[start..].split('"').next().expect("token").to_owned()
}

fn signed_stack() -> ComposeStack {
    let value: serde_yaml::Value =
        serde_yaml::from_str("services:\n  redis:\n    image: library/redis:7\n").expect("yaml");
    let canonical = serde_yaml::to_string(&value).expect("canonical");
    let key = SigningKey::from_bytes(&[
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ]);
    ComposeStack {
        id: uuid::Uuid::new_v4(),
        name: "cache".into(),
        signature_b64: base64::engine::general_purpose::STANDARD
            .encode(key.sign(canonical.as_bytes()).to_bytes()),
        compose_yaml_canonical: canonical,
        site_id: None,
    }
}

#[tokio::test]
async fn owner_container_rest_lifecycle_is_strict_and_bounded() {
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
    let token = server.login("owner", "correct horse battery staple").await;
    let id = uuid::Uuid::new_v4();
    let spec = serde_json::json!({"id":id,"name":"redis","image":"library/redis:7","site_id":null,"env":{"PASSWORD":"secret"},"ports":[],"mounts":[],"capabilities":[],"limits":{"cpu_shares":1024,"mem_mb":64,"pids_max":64},"restart_policy":"no","user_namespace":"1001:1001"});
    let base = format!("{}/api/v1/docker", server.base_url());
    let allow = server
        .client()
        .post(format!("{base}/images/allowlist"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"pattern":"library/redis:*","allow_pull":true,"pin_digest_required":false}))
        .send()
        .await
        .expect("allow");
    assert_eq!(allow.status(), 200);
    let allowlist = server
        .client()
        .get(format!("{base}/images/allowlist"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("allowlist");
    assert_eq!(allowlist.status(), 200);
    let pull = server
        .client()
        .post(format!("{base}/images/pull"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"image":"library/redis:7"}))
        .send()
        .await
        .expect("pull");
    assert_eq!(pull.status(), 200);
    let created = server
        .client()
        .post(format!("{base}/containers"))
        .bearer_auth(&token)
        .json(&spec)
        .send()
        .await
        .expect("create");
    assert_eq!(created.status(), 200);
    let body: serde_json::Value = created.json().await.expect("json");
    assert!(body.to_string().contains(&id.to_string()));
    assert!(!body.to_string().contains("secret"));
    let listed = server
        .client()
        .get(format!("{base}/containers"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list");
    assert_eq!(listed.status(), 200);
    let inspected = server
        .client()
        .get(format!("{base}/containers/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("inspect");
    assert_eq!(inspected.status(), 200);
    let started = server
        .client()
        .post(format!("{base}/containers/{id}/start"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("start");
    assert_eq!(started.status(), 200);
    for action in ["restart", "stop", "start"] {
        let response = server
            .client()
            .post(format!("{base}/containers/{id}/{action}"))
            .bearer_auth(&token)
            .send()
            .await
            .expect("lifecycle");
        assert_eq!(response.status(), 200, "{action}");
    }
    let logs = server
        .client()
        .get(format!("{base}/containers/{id}/logs?tail=10"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("logs");
    assert_eq!(logs.status(), 200);
    let exec = server
        .client()
        .post(format!("{base}/containers/{id}/exec"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"command":["echo","ok"]}))
        .send()
        .await
        .expect("exec");
    assert_eq!(exec.status(), 200);
    let stack = signed_stack();
    let stack_id = stack.id;
    let put_stack = server
        .client()
        .post(format!("{base}/stacks"))
        .bearer_auth(&token)
        .json(&stack)
        .send()
        .await
        .expect("put stack");
    assert_eq!(put_stack.status(), 200);
    let stacks = server
        .client()
        .get(format!("{base}/stacks"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("stacks");
    assert_eq!(stacks.status(), 200);
    let applied = server
        .client()
        .post(format!("{base}/stacks/{stack_id}/apply"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("apply stack");
    assert_eq!(applied.status(), 200);
    let deleted = server
        .client()
        .delete(format!("{base}/stacks/{stack_id}"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("delete stack");
    assert_eq!(deleted.status(), 200);
    let removed = server
        .client()
        .delete(format!("{base}/containers/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("remove");
    assert_eq!(removed.status(), 200);
}

#[tokio::test]
async fn docker_web_is_owner_only_and_mutations_require_csrf() {
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
    let page = server
        .client()
        .get(format!("{}/docker", server.base_url()))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("page");
    assert_eq!(page.status(), 200);
    let body = page.text().await.expect("html");
    assert!(body.contains("Forbidden capabilities"));
    let token = csrf(&body);
    let bad = server
        .client()
        .post(format!("{}/docker", server.base_url()))
        .header("cookie", &cookie)
        .form(&[("_csrf", "wrong"), ("spec_json", "{}")])
        .send()
        .await
        .expect("bad csrf");
    assert_eq!(bad.status(), 403);
    let owner = server
        .identity()
        .list_users()
        .await
        .expect("users")
        .into_iter()
        .find(|user| user.username().as_str() == "owner")
        .expect("owner");
    server
        .docker()
        .put_allowlist(
            &owner,
            ImageAllowlistEntry {
                pattern: "library/redis:*".into(),
                allow_pull: true,
                pin_digest_required: false,
            },
        )
        .await
        .expect("allow");
    let id = uuid::Uuid::new_v4();
    let spec = serde_json::json!({"id":id,"name":"webredis","image":"library/redis:7","site_id":null,"env":{},"ports":[],"mounts":[],"capabilities":[],"limits":{"cpu_shares":1024,"mem_mb":64,"pids_max":64},"restart_policy":"no","user_namespace":"1001:1001"}).to_string();
    let created = server
        .client()
        .post(format!("{}/docker", server.base_url()))
        .header("cookie", &cookie)
        .form(&[("_csrf", token.as_str()), ("spec_json", spec.as_str())])
        .send()
        .await
        .expect("web create");
    assert_eq!(created.status(), 200);
    let logs = server
        .client()
        .get(format!("{}/docker/{id}/logs", server.base_url()))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("web logs");
    assert_eq!(logs.status(), 200);
    assert!(
        logs.text()
            .await
            .expect("log html")
            .contains("Container logs")
    );
}
