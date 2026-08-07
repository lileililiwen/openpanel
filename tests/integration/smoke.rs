use crate::common::*;

/// Smoke test: the server boots and the health endpoint responds.
/// This is the "does the server even start?" gate.
#[tokio::test]
async fn smoke_server_boots_and_health() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/health", server.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

/// Every route is mounted under /api/v1 and returns 404/401 rather than
/// a 500, proving the router is wired.
#[tokio::test]
async fn smoke_router_mounted() {
    let server = TestServer::new().await;
    let client = server.client();
    let base = server.base_url();

    for path in [
        "/api/v1/identity/login",
        "/api/v1/identity/me",
        "/api/v1/identity/users",
        "/api/v1/sites",
        "/api/v1/databases",
        "/api/v1/files/00000000-0000-0000-0000-000000000000",
    ] {
        let resp = client.get(format!("{base}{path}")).send().await.unwrap();
        assert!(resp.status().as_u16() != 500, "route {path} returned 500");
    }
}
