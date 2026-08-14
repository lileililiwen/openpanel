//! i18n REST lifecycle (locales list, locale detail, translation
//! submission, and per-user preference set).
//!
//! The endpoints are intentionally read-heavy; the only mutation
//! is `POST /api/v1/locales/{locale}/translations` for community
//! translators (the panel accepts translations with a valid
//! publisher signature in v0.1; missing-signature translations
//! are stored but flagged in the audit log).

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use openpanel_app::{LocaleService, TranslationEntry, i18n::I18nAppError};
use openpanel_domain::common::Username;
use openpanel_domain::i18n::Locale;
use serde::{Deserialize, Serialize};

use crate::{
    error::{ApiError, ApiResult},
    extract::AuthUser,
};

/// Build routes nested at /api/v1/locales.
pub fn router(service: Arc<LocaleService>) -> Router {
    Router::new()
        .route("/locales", get(list_locales))
        .route("/locales/{locale}", get(get_locale))
        .route("/locales/{locale}/translations", post(submit_translation))
        .route("/locales/user/preference", post(set_user_preference))
        .with_state(service)
}

#[derive(Debug, Serialize)]
struct LocaleEntry {
    tag: String,
    language: String,
    region: Option<String>,
}

async fn list_locales(
    State(service): State<Arc<LocaleService>>,
) -> ApiResult<Json<Vec<LocaleEntry>>> {
    let entries: Vec<LocaleEntry> = service
        .supported()
        .iter()
        .map(|l| LocaleEntry {
            tag: l.to_string(),
            language: l.language().to_string(),
            region: l.region().map(|r| r.to_string()),
        })
        .collect();
    Ok(Json(entries))
}

#[derive(Debug, Serialize)]
struct LocaleDetail {
    tag: String,
    message_count: usize,
    sample_keys: Vec<String>,
}

async fn get_locale(
    State(service): State<Arc<LocaleService>>,
    Path(locale): Path<String>,
) -> ApiResult<Json<LocaleDetail>> {
    let locale = Locale::new(&locale).map_err(|err| ApiError::BadRequest(err.to_string()))?;
    let resolver = service;
    let count = resolver
        .resolve(&locale, "save_button")
        .map(|_| 1)
        .unwrap_or(0);
    Ok(Json(LocaleDetail {
        tag: locale.to_string(),
        message_count: count,
        sample_keys: vec!["save_button".to_string()],
    }))
}

#[derive(Debug, Deserialize)]
struct TranslationRequest {
    key: String,
    value: String,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    signature: Option<String>,
}

async fn submit_translation(
    State(service): State<Arc<LocaleService>>,
    Path(locale): Path<String>,
    Json(req): Json<TranslationRequest>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let locale = Locale::new(&locale).map_err(|err| ApiError::BadRequest(err.to_string()))?;
    let entry = TranslationEntry {
        key: req.key,
        value: req.value,
        context: req.context,
        signature: req.signature,
    };
    service
        .submit_translation(locale, entry)
        .map_err(|err: I18nAppError| ApiError::BadRequest(err.to_string()))?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"status": "accepted"})),
    ))
}

#[derive(Debug, Deserialize)]
struct SetUserPreferenceRequest {
    username: String,
    locale: String,
}

async fn set_user_preference(
    State(service): State<Arc<LocaleService>>,
    AuthUser(user): AuthUser,
    Json(req): Json<SetUserPreferenceRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if req.username != user.username {
        return Err(ApiError::Forbidden);
    }
    let locale = Locale::new(&req.locale).map_err(|err| ApiError::BadRequest(err.to_string()))?;
    let username = Username::new(&req.username)
        .map_err(|err| ApiError::BadRequest(err.to_string()))?;
    let mut args = BTreeMap::new();
    args.insert("locale".to_string(), locale.to_string());
    let _ = args; // intentionally unused; placeholder for translator-supplied args
    service
        .set_user_locale(username, locale)
        .await
        .map_err(|err| ApiError::Internal(err.to_string()))?;
    Ok(Json(serde_json::json!({"status": "ok"})))
}
