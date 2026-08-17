//! Plugin marketplace HTTP routes.
//!
//! Three endpoints:
//!
//! * `GET  /marketplace/plugins` — list the latest cached catalog
//!   (verifying on the fly if the cache is empty).
//! * `GET  /marketplace/plugins/{id}` — fetch the cached detail for
//!   one plugin (including rating, summary, publisher).
//! * `POST /marketplace/plugins/{id}/install` — install a plugin
//!   from the marketplace catalog. The marketplace layer verifies
//!   the publisher signature and delegates the actual install to
//!   the base plugin service.
//!
//! The endpoints live under `/marketplace` so they stay
//! discoverable alongside the other plugin routes. The base plugin
//! routes (`/plugins`) are intentionally out of scope for this
//! refinement change and ship separately.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use ed25519_dalek::VerifyingKey;
use openpanel_app::plugin_marketplace::{
    InstallFromMarketplaceError, InstallFromMarketplaceRequest, MarketplaceService,
};
use openpanel_domain::{
    MarketplaceCatalog, MarketplacePlugin, PluginMarketplaceError, PluginRating,
    SignedCatalogEnvelope,
};
use serde::{Deserialize, Serialize};

use crate::{ApiError, ApiResult, AuthUser};

/// Build the `/marketplace` routes.
pub fn router(service: Arc<MarketplaceService>) -> Router {
    Router::new()
        .route("/plugins", get(list_plugins))
        .route("/plugins/{id}", get(plugin_detail))
        .route("/plugins/{id}/install", axum::routing::post(install))
        .with_state(service)
}

async fn list_plugins(
    State(svc): State<Arc<MarketplaceService>>,
    AuthUser(_user, _session): AuthUser,
) -> ApiResult<Json<MarketplaceView>> {
    // Prefer cache; if absent, attempt one discovery round-trip.
    let cached = svc.cached().await.map_err(map_marketplace_error)?;
    let snapshot = match cached {
        Some(s) => s,
        None => {
            // We don't fetch here — that's the discover CLI route's
            // job. Surface a 503 so the caller knows to discover
            // first.
            return Err(ApiError::ServiceUnavailable(
                "marketplace catalog not yet discovered".into(),
            ));
        }
    };
    Ok(Json(MarketplaceView::from(&snapshot.catalog)))
}

async fn plugin_detail(
    State(svc): State<Arc<MarketplaceService>>,
    AuthUser(_user, _session): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<PluginDetailView>> {
    let cached = svc.cached().await.map_err(map_marketplace_error)?;
    let snapshot = cached.ok_or_else(|| {
        ApiError::ServiceUnavailable("marketplace catalog not yet discovered".into())
    })?;
    let entry = snapshot
        .catalog
        .find(&id)
        .ok_or_else(|| ApiError::NotFound(format!("plugin {id} not in catalog")))?;
    Ok(Json(PluginDetailView::from(entry)))
}

#[derive(Debug, Deserialize)]
struct InstallBody {
    manifest: SignedManifestBody,
    publisher_key_b64: String,
}

#[derive(Debug, Deserialize)]
struct SignedManifestBody {
    id: String,
    version: String,
    runtime: String,
    entrypoint: String,
    #[serde(default)]
    permissions: Vec<String>,
    publisher: String,
    signature: String,
}

async fn install(
    State(svc): State<Arc<MarketplaceService>>,
    AuthUser(user, _session): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<InstallBody>,
) -> ApiResult<(StatusCode, Json<PluginDetailView>)> {
    let key_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        body.publisher_key_b64.as_bytes(),
    )
    .map_err(|_| ApiError::BadRequest("publisher key not valid base64".into()))?;
    let key_arr: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ApiError::BadRequest("publisher key must be 32 bytes".into()))?;
    let verifying_key = VerifyingKey::from_bytes(&key_arr)
        .map_err(|_| ApiError::BadRequest("publisher key not a valid ed25519 point".into()))?;
    let manifest = openpanel_domain::PluginManifest {
        id: openpanel_domain::PluginId::new(&body.manifest.id)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?,
        version: openpanel_domain::PluginVersion::new(&body.manifest.version)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?,
        runtime: match body.manifest.runtime.as_str() {
            "json-rpc" => openpanel_domain::ManifestRuntime::JsonRpc,
            "wasm" => openpanel_domain::ManifestRuntime::Wasm,
            other => {
                return Err(ApiError::BadRequest(format!("unknown runtime: {other}")));
            }
        },
        entrypoint: body.manifest.entrypoint.clone(),
        capabilities: openpanel_domain::CapabilitySet::default(),
        permissions: body.manifest.permissions.clone(),
        ui: openpanel_domain::plugin::manifest::PluginUi::default(),
        publisher: openpanel_domain::PublisherKey::new(&body.manifest.publisher)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?,
        signature: body.manifest.signature.clone(),
        created_at: None,
    };
    let request = InstallFromMarketplaceRequest {
        plugin_id: id.clone(),
        manifest_url: "marketplace://install".into(),
        manifest,
        publisher_id: body.manifest.publisher.clone(),
    };
    svc.install_from_marketplace(request, &verifying_key, user.username().as_str())
        .await
        .map_err(map_install_error)?;
    // Return a minimal detail view; the catalog may not have this
    // entry cached, so fabricate one.
    let detail = PluginDetailView {
        id,
        name: body.manifest.id.clone(),
        publisher: body.manifest.publisher.clone(),
        manifest_url: "marketplace://install".into(),
        rating: 0.0,
        summary: String::new(),
    };
    Ok((StatusCode::CREATED, Json(detail)))
}

fn map_install_error(e: InstallFromMarketplaceError) -> ApiError {
    match e {
        InstallFromMarketplaceError::Marketplace(PluginMarketplaceError::PublisherUnverified) => {
            ApiError::Forbidden
        }
        InstallFromMarketplaceError::Marketplace(PluginMarketplaceError::ManifestTampered) => {
            ApiError::BadRequest("manifest tampered".into())
        }
        InstallFromMarketplaceError::Marketplace(PluginMarketplaceError::ManifestUrlMismatch(
            id,
        )) => ApiError::BadRequest(format!("manifest url mismatch for {id}")),
        InstallFromMarketplaceError::Marketplace(PluginMarketplaceError::InvalidCatalog(msg)) => {
            ApiError::BadRequest(msg)
        }
        InstallFromMarketplaceError::Marketplace(other) => {
            ApiError::ServiceUnavailable(other.to_string())
        }
        InstallFromMarketplaceError::Plugin(e) => ApiError::ServiceUnavailable(e.to_string()),
    }
}

fn map_marketplace_error(e: PluginMarketplaceError) -> ApiError {
    ApiError::ServiceUnavailable(e.to_string())
}

// ---- Views ----

#[derive(Debug, Serialize)]
struct MarketplaceView {
    entries: Vec<PluginDetailView>,
    publisher_id: String,
    expires_at: u64,
}

impl From<&MarketplaceCatalog> for MarketplaceView {
    fn from(c: &MarketplaceCatalog) -> Self {
        Self {
            entries: c.entries.iter().map(PluginDetailView::from).collect(),
            publisher_id: c.publisher_id.clone(),
            expires_at: c.expires_at,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
struct PluginDetailView {
    id: String,
    name: String,
    publisher: String,
    manifest_url: String,
    rating: f32,
    summary: String,
}

impl From<&MarketplacePlugin> for PluginDetailView {
    fn from(p: &MarketplacePlugin) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            publisher: p.publisher.clone(),
            manifest_url: p.manifest_url.clone(),
            rating: PluginRating::new(p.rating)
                .map(|r| r.as_f32())
                .unwrap_or(0.0),
            summary: p.summary.clone(),
        }
    }
}

// Suppress unused warning when the type is constructed in tests.
#[allow(dead_code)]
fn _signed_envelope_view(e: &SignedCatalogEnvelope) -> serde_json::Value {
    serde_json::json!({
        "schema": e.schema,
        "expires_at": e.expires_at,
        "publisher_id": e.publisher_id,
    })
}
