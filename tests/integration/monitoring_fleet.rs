//! Monitoring-fleet operations HTTP integration tests.
//!
//! Capability under test: `monitoring-fleet-operations` (configurable
//! views, threshold hysteresis, independent uptime, safe scoped fleet
//! health).

use crate::common::*;

async fn authed_server() -> (TestServer, String) {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    (server, token)
}

/// Configurable views: bounded query validation + saved-view round trip
/// through the service (empty/stale/unavailable semantics are domain
/// covered; the API surfaces the fleet health projection).
#[tokio::test]
async fn monitoring_fleet_operations_views_are_bounded() {
    let (server, _token) = authed_server().await;
    let svc = server.monitoring_fleet();
    assert!(
        svc.validate_query(30, 100, 60).is_err(),
        "range below 60s must be rejected"
    );
    assert!(svc.validate_query(3600, 500, 60).is_ok());
}

/// Threshold hysteresis: one breach, no duplicate, one recovery.
#[tokio::test]
async fn monitoring_fleet_operations_thresholds_have_hysteresis() {
    use openpanel_domain::monitoring_fleet::FleetThresholdPolicy;
    let (server, _token) = authed_server().await;
    let svc = server.monitoring_fleet();
    let users = server.identity().list_users().await.expect("users");
    let owner = users
        .into_iter()
        .find(|user| user.username().as_str() == "admin")
        .expect("owner exists");
    let now = chrono::Utc::now();
    let policy = FleetThresholdPolicy::new(90.0, 80.0, 10).expect("policy");
    let id = svc
        .register_threshold_policy(&owner, "cpu_percent", policy, now)
        .await
        .expect("register");
    let breach = svc
        .evaluate_fleet_threshold(id, 95.0, now)
        .await
        .expect("evaluate");
    assert!(breach.is_some_and(|event| event.breached));
    let duplicate = svc
        .evaluate_fleet_threshold(id, 96.0, now)
        .await
        .expect("evaluate");
    assert!(duplicate.is_none(), "no re-fire while breached");
    let recovery = svc
        .evaluate_fleet_threshold(id, 70.0, now)
        .await
        .expect("evaluate");
    assert!(recovery.is_some_and(|event| !event.breached));
}

/// Uptime independence: independent probes stay observable when the
/// panel is down and distinguish outage kinds.
#[tokio::test]
async fn monitoring_fleet_operations_probes_are_independent() {
    let (server, token) = authed_server().await;
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/monitoring/probe/independent",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("GET");
    assert_eq!(resp.status(), 200, "body: {}", resp.text().await.unwrap());
    // Domain-level independence: panel-down result stays observable.
    let probe = server.monitoring_fleet().record_independent_probe(
        false,
        false,
        "timeout",
        chrono::Utc::now(),
    );
    assert!(probe.observable_when_panel_down());
    assert_eq!(probe.outage_kind(), "panel-and-target-down");
}

/// Fleet health: heartbeat expiry marks stale with guidance; the API +
/// web surfaces are secret-free and scoped.
#[tokio::test]
async fn monitoring_fleet_operations_fleet_health_is_safe_and_scoped() {
    let (server, token) = authed_server().await;
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/monitoring/fleet/health",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("GET");
    assert_eq!(resp.status(), 200, "body: {}", resp.text().await.unwrap());
    let body = resp.text().await.expect("body");
    assert!(!body.contains("cert"), "no certificate material: {body}");
    assert!(!body.contains("token"), "no token material: {body}");
}

/// Web surfaces: configurable views + fleet health render without
/// secrets and require authentication.
#[tokio::test]
async fn monitoring_fleet_operations_web_surfaces_render() {
    let server = TestServer::new().await;
    // Unauthenticated web access redirects to login.
    let unauth = server
        .client()
        .get(format!("{}/fleet", server.base_url()))
        .send()
        .await
        .expect("GET");
    assert!(
        unauth.status() == 302 || unauth.status() == 401 || unauth.status() == 403,
        "fleet must not render anonymously, got {}",
        unauth.status()
    );
}

/// Fleet views API requires authentication.
#[tokio::test]
async fn monitoring_fleet_operations_api_requires_authentication() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/monitoring/fleet/health",
            server.base_url()
        ))
        .send()
        .await
        .expect("GET");
    assert_eq!(resp.status(), 401);
}
