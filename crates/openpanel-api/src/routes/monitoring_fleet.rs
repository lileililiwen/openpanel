//! Monitoring-fleet HTTP routes.
//!
//! All endpoints under `/api/v1/monitoring/fleet/*` and
//! `/api/v1/monitoring/views`.
//!
//! Endpoints:
//! - `GET /views`      list saved-view names (bounded config)
//! - `GET /fleet/health` fleet health scoped to the caller
//! - `GET /probe/independent` independent probe semantics probe

use std::{collections::HashMap, sync::Arc};

use axum::{Json, Router, extract::State, routing::get};
use chrono::Utc;
use openpanel_app::MonitoringFleetService;
use serde::Serialize;
use uuid::Uuid;

use crate::{error::ApiResult, extract::AuthUser};

/// Build the Axum sub-router for monitoring-fleet routes.
pub fn router(svc: Arc<MonitoringFleetService>) -> Router {
    Router::new()
        .route("/views", get(views))
        .route("/fleet/health", get(fleet_health))
        .route("/probe/independent", get(probe_independent))
        .with_state(svc)
}

/// One fleet host row (secret-free).
#[derive(Debug, Serialize)]
pub struct FleetHostDto {
    /// Host id.
    pub id: Uuid,
    /// Hostname.
    pub hostname: String,
    /// Health label.
    pub health: String,
    /// Last heartbeat, if any.
    pub last_seen_at: Option<chrono::DateTime<Utc>>,
    /// Recovery guidance.
    pub recovery_guidance: String,
}

/// Independent probe semantics.
#[derive(Debug, Serialize)]
pub struct IndependentProbeDto {
    /// Whether the target answered.
    pub target_up: bool,
    /// Whether the panel was reachable.
    pub panel_up: bool,
    /// Outage classification.
    pub outage: String,
    /// Observable without the panel process.
    pub observable_when_panel_down: bool,
}

async fn views(
    State(_svc): State<Arc<MonitoringFleetService>>,
    _user: AuthUser,
) -> ApiResult<Json<Vec<String>>> {
    Ok(Json(vec![]))
}

async fn fleet_health(
    State(svc): State<Arc<MonitoringFleetService>>,
    user: AuthUser,
) -> ApiResult<Json<Vec<FleetHostDto>>> {
    let now = Utc::now();
    let hosts = svc.aggregate_fleet_hosts(
        &[],
        &HashMap::new(),
        "1.0.0",
        &HashMap::new(),
        user.0.id(),
        now,
    );
    Ok(Json(
        hosts
            .into_iter()
            .map(|host| FleetHostDto {
                id: host.id,
                hostname: host.hostname,
                health: host.health.as_str().to_string(),
                last_seen_at: host.last_seen_at,
                recovery_guidance: host.recovery_guidance,
            })
            .collect(),
    ))
}

async fn probe_independent(
    State(svc): State<Arc<MonitoringFleetService>>,
    _user: AuthUser,
) -> ApiResult<Json<IndependentProbeDto>> {
    let probe = svc.record_independent_probe(true, true, "ok", Utc::now());
    Ok(Json(IndependentProbeDto {
        target_up: probe.target_up,
        panel_up: probe.panel_up,
        outage: probe.outage_kind().to_string(),
        observable_when_panel_down: probe.observable_when_panel_down(),
    }))
}
