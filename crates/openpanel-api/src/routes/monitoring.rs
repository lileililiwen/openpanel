//! Monitoring HTTP routes.
//!
//! All endpoints under `/api/v1/monitoring/*`.
//!
//! Endpoints:
//! - `GET /overview`   current snapshot
//! - `GET /history`    time series for one metric (`?metric=&range=`)
//! - `GET /alerts`     recent `AlertFired` audit events

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use chrono::{Duration, Utc};
use openpanel_app::MonitoringService;
use openpanel_core::AuditAction;
use openpanel_domain::monitoring::SystemSnapshot;
use serde::{Deserialize, Serialize};

use crate::{
    dto::{HistoryPointDto, OverviewDto},
    error::{ApiError, ApiResult},
    extract::AuthUser,
};

/// Build the Axum sub-router for `/monitoring` routes.
pub fn router(svc: Arc<MonitoringService>) -> Router {
    Router::new()
        .route("/overview", get(overview))
        .route("/history", get(history))
        .route("/alerts", get(alerts))
        .with_state(svc)
}

/// Query params for `GET /monitoring/history`.
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Metric kind identifier (`Cpu`, `Memory`, `Disk`, `Network`).
    pub metric: String,
    /// Look-back window in seconds (default 3600).
    pub range: Option<i64>,
}

/// Query params for `GET /monitoring/alerts`.
#[derive(Debug, Deserialize)]
pub struct AlertsQuery {
    /// Maximum number of events to return (default 20).
    pub limit: Option<i64>,
}

async fn overview(
    State(svc): State<Arc<MonitoringService>>,
    _user: AuthUser,
) -> ApiResult<Json<OverviewDto>> {
    // Collect a fresh snapshot so `/overview` always reflects the
    // current host, even if the collector task has not ticked yet.
    let snapshot: SystemSnapshot = svc.snapshot_now().await?;
    Ok(Json(OverviewDto::from(&snapshot)))
}

async fn history(
    State(svc): State<Arc<MonitoringService>>,
    _user: AuthUser,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<Vec<HistoryPointDto>>> {
    let kind = q
        .metric
        .parse()
        .map_err(|e: openpanel_domain::monitoring::MonitoringError| ApiError::Monitoring(e))?;
    let range = Duration::seconds(q.range.unwrap_or(3600));
    let since = Utc::now() - range;
    let samples = svc.history(kind, since).await?;
    Ok(Json(samples.iter().map(HistoryPointDto::from).collect()))
}

/// A recent alert event from the audit log.
#[derive(Debug, Serialize)]
pub struct AlertDto {
    /// When the alert fired.
    pub timestamp: chrono::DateTime<Utc>,
    /// Metric that crossed its threshold.
    pub metric: String,
    /// Measured value at firing time.
    pub value: f64,
    /// Configured threshold.
    pub threshold: f64,
}

async fn alerts(
    State(svc): State<Arc<MonitoringService>>,
    _user: AuthUser,
    Query(q): Query<AlertsQuery>,
) -> ApiResult<Json<Vec<AlertDto>>> {
    let limit = q.limit.unwrap_or(20).clamp(1, 500);
    // The audit service is the single source of truth for alert events;
    // filter the newest events for the `AlertFired` action.
    let events = svc.audit_events(limit).await?;
    let mut out = Vec::new();
    for event in events {
        if event.action != AuditAction::AlertFired {
            continue;
        }
        let metric = event.target.unwrap_or_default();
        let value = event
            .metadata
            .get("value")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let threshold = event
            .metadata
            .get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        out.push(AlertDto {
            timestamp: event.ts,
            metric,
            value,
            threshold,
        });
    }
    Ok(Json(out))
}
