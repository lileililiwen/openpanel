//! SSL HTTP routes.
//!
//! All endpoints under `/api/v1/ssl/*`. Private-key material is
//! NEVER returned in any response — only metadata.
//!
//! Endpoints:
//! - `GET    /certificates`                          list all
//! - `GET    /certificates/{domain}`                 fetch one
//! - `POST   /certificates/acme`                     ACME HTTP-01 issue
//! - `POST   /certificates/manual`                   upload PEM
//! - `POST   /certificates/self-signed`              self-signed
//! - `DELETE /certificates/{domain}`                 revoke + remove
//! - `POST   /certificates/{domain}/renew`            force-renew
//! - `PATCH  /certificates/{domain}/force-https`      toggle 301

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use openpanel_app::SslService;
use openpanel_domain::ssl::source::CertificateSource;

use crate::{
    dto::{
        CertificateListResponse, CertificateMetadataDto, CertificateResponse, ForceHttpsRequest,
        IssueRequest, ManualUploadRequest, SelfSignedRequest,
    },
    error::{ApiError, ApiResult},
    extract::AuthUser,
};

/// Build the Axum sub-router for `/ssl` routes.
pub fn router(svc: Arc<SslService>) -> Router {
    Router::new()
        .route("/certificates", get(list_certificates))
        .route("/certificates/acme", post(issue_acme))
        .route("/certificates/manual", post(upload_manual))
        .route("/certificates/self-signed", post(generate_self_signed))
        .route("/certificates/{domain}", get(get_certificate))
        .route("/certificates/{domain}", delete(delete_certificate))
        .route("/certificates/{domain}/renew", post(renew_certificate))
        .route("/certificates/{domain}/force-https", patch(set_force_https))
        .with_state(svc)
}

async fn list_certificates(
    State(svc): State<Arc<SslService>>,
    _user: AuthUser,
) -> ApiResult<Json<CertificateListResponse>> {
    let certs = svc.list().await?;
    Ok(Json(CertificateListResponse {
        certificates: certs
            .into_iter()
            .map(CertificateMetadataDto::from)
            .collect(),
    }))
}

async fn get_certificate(
    State(svc): State<Arc<SslService>>,
    _user: AuthUser,
    Path(domain): Path<String>,
) -> ApiResult<Json<CertificateResponse>> {
    let cert = svc.get(&domain).await?;
    Ok(Json(CertificateResponse {
        certificate: CertificateMetadataDto::from(cert),
    }))
}

async fn issue_acme(
    State(svc): State<Arc<SslService>>,
    _user: AuthUser,
    Json(req): Json<IssueRequest>,
) -> ApiResult<Json<CertificateResponse>> {
    if let Some(ep) = &req.endpoint
        && !matches!(ep.as_str(), "staging" | "production")
    {
        return Err(ApiError::BadRequest(format!(
            "endpoint must be 'staging' or 'production', got `{ep}`"
        )));
    }
    let cert = svc.issue_acme(&req.domain).await?;
    Ok(Json(CertificateResponse {
        certificate: CertificateMetadataDto::from(cert),
    }))
}

async fn upload_manual(
    State(svc): State<Arc<SslService>>,
    _user: AuthUser,
    Json(req): Json<ManualUploadRequest>,
) -> ApiResult<Json<CertificateResponse>> {
    let cert = svc
        .upload_manual(&req.domain, &req.cert_pem, &req.chain_pem, &req.key_pem)
        .await?;
    Ok(Json(CertificateResponse {
        certificate: CertificateMetadataDto::from(cert),
    }))
}

async fn generate_self_signed(
    State(svc): State<Arc<SslService>>,
    _user: AuthUser,
    Json(req): Json<SelfSignedRequest>,
) -> ApiResult<Json<CertificateResponse>> {
    let cert = svc
        .generate_self_signed(&req.domain, req.valid_for_days)
        .await?;
    Ok(Json(CertificateResponse {
        certificate: CertificateMetadataDto::from(cert),
    }))
}

async fn delete_certificate(
    State(svc): State<Arc<SslService>>,
    _user: AuthUser,
    Path(domain): Path<String>,
) -> ApiResult<StatusCode> {
    svc.delete(&domain).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn renew_certificate(
    State(svc): State<Arc<SslService>>,
    _user: AuthUser,
    Path(domain): Path<String>,
) -> ApiResult<Json<CertificateResponse>> {
    let cert = svc.renew_now(&domain).await?;
    Ok(Json(CertificateResponse {
        certificate: CertificateMetadataDto::from(cert),
    }))
}

async fn set_force_https(
    State(svc): State<Arc<SslService>>,
    _user: AuthUser,
    Path(domain): Path<String>,
    Json(req): Json<ForceHttpsRequest>,
) -> ApiResult<Json<CertificateResponse>> {
    let cert = svc.set_force_https(&domain, req.enabled).await?;
    Ok(Json(CertificateResponse {
        certificate: CertificateMetadataDto::from(cert),
    }))
}

// Silence unused warning for the re-export anchor.
#[allow(dead_code)]
const _: Option<CertificateSource> = None;
