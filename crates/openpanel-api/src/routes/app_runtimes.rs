//! Per-runtime environment routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use openpanel_app::RuntimeEnvService;
use openpanel_domain::app_runtimes::EnvVar;
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Builds the Axum sub-router for runtime environments.
pub fn router(svc: Arc<RuntimeEnvService>) -> Router {
    Router::new()
        .route("/{id}/env", get(get_env).put(put_env))
        .with_state(svc)
}

async fn get_env(
    State(svc): State<Arc<RuntimeEnvService>>,
    AuthUser(caller, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let views = svc.get(&caller, id).await.map_err(map_runtime)?;
    Ok(Json(
        serde_json::to_value(&views).map_err(|e| ApiError::Internal(e.to_string()))?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvInput {
    vars: Vec<EnvVarInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvVarInput {
    key: String,
    value: String,
    secret: bool,
}

async fn put_env(
    State(svc): State<Arc<RuntimeEnvService>>,
    AuthUser(caller, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<EnvInput>,
) -> ApiResult<axum::http::StatusCode> {
    let mut vars = Vec::new();
    for var in input.vars {
        vars.push(
            EnvVar::new(var.key, var.value, var.secret)
                .map_err(|e| ApiError::Unprocessable(e.to_string()))?,
        );
    }
    let set = openpanel_domain::app_runtimes::EnvSet::new(vars)
        .map_err(|e| ApiError::Unprocessable(e.to_string()))?;
    svc.set(&caller, id, set).await.map_err(map_runtime)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

fn map_runtime(error: openpanel_domain::app_runtimes::RuntimeError) -> ApiError {
    use openpanel_domain::app_runtimes::RuntimeError;
    match error {
        RuntimeError::Forbidden => ApiError::Forbidden,
        RuntimeError::Persistence(_) => ApiError::Internal(error.to_string()),
        other => ApiError::Unprocessable(other.to_string()),
    }
}
