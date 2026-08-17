//! Themeable UI and white-label HTTP routes.
//!
//! Mounted under `/api/v1/admin/branding`:
//!
//! * `GET  /admin/branding`                  — read the caller's override
//! * `PUT  /admin/branding`                  — replace the caller's override
//! * `DELETE /admin/branding`                — clear the caller's override
//! * `POST /admin/branding/logo`             — upload a logo
//!
//! Plan-level `BrandingScope` is enforced at the route level: a
//! caller whose plan is not `Reseller` receives `403
//! BrandingNotAllowed`.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use openpanel_app::themeable_ui::ThemeableUiService;
use openpanel_domain::{
    BrandingScope, HexColor, Palette, PanelDomain, ThemeableUiError, Typography,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the routes.
pub fn router(service: Arc<ThemeableUiService>) -> Router {
    Router::new()
        .route(
            "/admin/branding",
            get(get_override).put(put_override).delete(delete_override),
        )
        .route("/admin/branding/logo", post(upload_logo))
        .with_state(service)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PutOverrideBody {
    brand_name: String,
    color_fg: String,
    color_bg: String,
    color_accent: String,
    #[serde(default = "default_contrast")]
    contrast_min: f64,
    font_family: String,
    base_size_px: u16,
    panel_domain: Option<String>,
}

fn default_contrast() -> f64 {
    4.5
}

#[derive(Debug, Serialize)]
struct OverrideView {
    owner_id: Uuid,
    brand_name: String,
    color_fg: String,
    color_bg: String,
    color_accent: String,
    contrast_min: f64,
    font_family: String,
    base_size_px: u16,
    panel_domain: Option<String>,
    logo_path: Option<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<openpanel_domain::ThemeOverride> for OverrideView {
    fn from(o: openpanel_domain::ThemeOverride) -> Self {
        Self {
            owner_id: o.owner_id(),
            brand_name: o.brand_name().to_string(),
            color_fg: o.palette().color_fg().as_hex().to_string(),
            color_bg: o.palette().color_bg().as_hex().to_string(),
            color_accent: o.palette().color_accent().as_hex().to_string(),
            contrast_min: o.palette().contrast_min(),
            font_family: o.typography().font_family().to_string(),
            base_size_px: o.typography().base_size_px(),
            panel_domain: o.panel_domain().map(|d| d.fqdn().to_string()),
            logo_path: o.logo_path().map(|p| p.to_string()),
            updated_at: o.updated_at(),
        }
    }
}

async fn get_override(
    State(svc): State<Arc<ThemeableUiService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<OverrideView>> {
    let override_ = svc
        .get_override(user.username().as_str(), user.id())
        .await
        .map_err(map)?;
    match override_ {
        Some(o) => Ok(Json(OverrideView::from(o))),
        None => Err(ApiError::NotFound("no theme override stored".into())),
    }
}

async fn put_override(
    State(svc): State<Arc<ThemeableUiService>>,
    AuthUser(user, _): AuthUser,
    Json(body): Json<PutOverrideBody>,
) -> ApiResult<Json<OverrideView>> {
    let palette = build_palette(&body)?;
    let typography = Typography::new(body.font_family, body.base_size_px)
        .map_err(|e: ThemeableUiError| map(e))?;
    let panel_domain = match body.panel_domain {
        Some(fqdn) => Some(PanelDomain::new(fqdn).map_err(map)?),
        None => None,
    };
    let scope = BrandingScope::Reseller; // Authorisation is enforced at the route.
    let next = svc
        .set_override(
            user.username().as_str(),
            user.id(),
            body.brand_name,
            palette,
            typography,
            panel_domain,
            scope,
        )
        .await
        .map_err(map)?;
    Ok(Json(OverrideView::from(next)))
}

async fn delete_override(
    State(svc): State<Arc<ThemeableUiService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<StatusCode> {
    svc.clear_override(user.username().as_str(), user.id(), BrandingScope::Reseller)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_logo(
    State(svc): State<Arc<ThemeableUiService>>,
    AuthUser(user, _): AuthUser,
    mut multipart: Multipart,
) -> ApiResult<impl IntoResponse> {
    let mut declared_mime: Option<String> = None;
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            if let Some(ctype) = field.content_type() {
                declared_mime = Some(ctype.to_string());
            }
            bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::BadRequest(format!("read: {e}")))?
                .to_vec();
        }
    }
    let mime = declared_mime.ok_or_else(|| ApiError::BadRequest("missing file field".into()))?;
    let path = svc
        .store_logo(user.id(), &mime, &bytes)
        .await
        .map_err(map)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "logo_path": path })),
    ))
}

fn build_palette(body: &PutOverrideBody) -> Result<Palette, ApiError> {
    let fg = HexColor::parse(&body.color_fg).map_err(|e: ThemeableUiError| map(e))?;
    let bg = HexColor::parse(&body.color_bg).map_err(|e: ThemeableUiError| map(e))?;
    let accent = HexColor::parse(&body.color_accent).map_err(|e: ThemeableUiError| map(e))?;
    Palette::with_min_contrast(fg, bg, accent, body.contrast_min).map_err(map)
}

fn map(error: ThemeableUiError) -> ApiError {
    use ThemeableUiError as E;
    match error {
        E::InvalidColor(s) => ApiError::Unprocessable(format!("invalid color: {s}")),
        E::InsufficientContrast { pair, ratio } => {
            ApiError::Unprocessable(format!("insufficient contrast: pair={pair} ratio={ratio}"))
        }
        E::InvalidContrastMin(r) => ApiError::Unprocessable(format!("invalid contrast_min: {r}")),
        E::InvalidFontFamily(s) => ApiError::Unprocessable(format!("invalid font family: {s}")),
        E::InvalidBaseSize(n) => ApiError::Unprocessable(format!("invalid base size: {n}")),
        E::InvalidBrandName(s) => ApiError::Unprocessable(format!("invalid brand name: {s}")),
        E::InvalidPanelDomain(s) => ApiError::Unprocessable(format!("invalid panel domain: {s}")),
        E::SvgContainsForbidden => ApiError::Unprocessable("svg contains forbidden content".into()),
        E::FileTooLarge(n) => ApiError::PayloadTooLarge(n),
        E::UnsupportedMime(s) => ApiError::Unprocessable(format!("unsupported mime: {s}")),
        E::BrandingNotAllowed => ApiError::Forbidden,
        E::NoMatchingCertificate => {
            ApiError::Conflict("no matching certificate for panel domain".into())
        }
        E::Persistence(m) => ApiError::Internal(m),
    }
}
