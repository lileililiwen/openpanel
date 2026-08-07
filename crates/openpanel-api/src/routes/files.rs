//! File HTTP routes — mounted under `/api/v1/files/{site_id}`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State, multipart::Multipart};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, patch, post};
use axum::{Json, Router};
use openpanel_app::FilesService;
use openpanel_domain::files::error::FileError;
use openpanel_domain::files::path::Path as FilePath;
use serde_json::json;
use uuid::Uuid;

use crate::dto::{FileInfoDto, ListDirResponse, RemoveRequest};
use crate::error::{ApiError, ApiResult};
use crate::extract::AuthUser;

pub fn router(svc: Arc<FilesService>) -> Router {
    Router::new()
        .route("/{site_id}", axum::routing::get(list_root))
        .route("/{site_id}/{*path}", axum::routing::get(list_or_read))
        .route("/{site_id}/{*path}", post(upload))
        .route("/{site_id}/{*path}", axum::routing::put(write_raw))
        .route("/{site_id}/{*path}", delete(remove))
        .route("/{site_id}/{*path}", patch(rename_or_chmod))
        .with_state(svc)
}

async fn list_root(
    State(svc): State<Arc<FilesService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Response> {
    let rel = FilePath::root();
    let entries = svc
        .list_dir(&user, site_id, &rel)
        .await
        .map_err(map_file_err)?;
    let body = Json(ListDirResponse {
        entries: entries.iter().map(FileInfoDto::from_info).collect(),
    });
    Ok(body.into_response())
}

async fn list_or_read(
    State(svc): State<Arc<FilesService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, path)): Path<(Uuid, String)>,
) -> ApiResult<Response> {
    let rel = FilePath::new(path).map_err(map_file_err)?;
    let chroot = svc.chroot_for(&user, site_id).await.map_err(map_file_err)?;
    let abs = openpanel_app::files::repo::resolve_path(&chroot, &rel)
        .await
        .map_err(map_file_err)?;
    let meta = match tokio::fs::metadata(&abs).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ApiError::NotFound(format!("{rel} not found")));
        }
        Err(e) => return Err(ApiError::Internal(e.to_string())),
    };
    if meta.is_dir() {
        let entries = svc
            .repo()
            .list_dir(&chroot, &rel)
            .await
            .map_err(map_file_err)?;
        let body = Json(ListDirResponse {
            entries: entries.iter().map(FileInfoDto::from_info).collect(),
        });
        return Ok(body.into_response());
    }
    // File: read raw bytes
    let (bytes, _mtime) = svc
        .repo()
        .read_file(&chroot, &rel)
        .await
        .map_err(map_file_err)?;
    let mime = guess_mime(&rel);
    let resp = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .body(axum::body::Body::from(bytes))
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(resp)
}

fn guess_mime(rel: &FilePath) -> String {
    let s = rel.as_str();
    let ext = std::path::Path::new(s)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let guess = match ext.to_lowercase().as_str() {
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "txt" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    };
    guess.to_string()
}

async fn upload(
    State(svc): State<Arc<FilesService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, path)): Path<(Uuid, String)>,
    content_type: axum::http::HeaderMap,
    mut multipart: Multipart,
) -> ApiResult<Response> {
    let ct = content_type
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !ct.starts_with("multipart/form-data") {
        return Err(ApiError::BadRequest(
            "POST to a path requires multipart/form-data (for upload)".into(),
        ));
    }
    let rel = FilePath::new(path).map_err(map_file_err)?;
    let mut bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        if field.name() == Some("file") {
            bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?
                    .to_vec(),
            );
        }
    }
    let bytes = bytes.ok_or_else(|| ApiError::BadRequest("missing `file` field".into()))?;
    svc.write_file(&user, site_id, &rel, &bytes)
        .await
        .map_err(map_file_err)?;
    Ok(Json(json!({"ok": true, "bytes": bytes.len()})).into_response())
}

async fn write_raw(
    State(svc): State<Arc<FilesService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, path)): Path<(Uuid, String)>,
    body: Bytes,
) -> ApiResult<Response> {
    let rel = FilePath::new(path).map_err(map_file_err)?;
    svc.write_file(&user, site_id, &rel, &body)
        .await
        .map_err(map_file_err)?;
    Ok(Json(json!({"ok": true, "bytes": body.len()})).into_response())
}

async fn remove(
    State(svc): State<Arc<FilesService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, path)): Path<(Uuid, String)>,
    axum::extract::Query(q): axum::extract::Query<RemoveRequest>,
) -> ApiResult<Response> {
    let rel = FilePath::new(path).map_err(map_file_err)?;
    svc.remove(&user, site_id, &rel, q.recursive)
        .await
        .map_err(map_file_err)?;
    Ok(Json(json!({"ok": true})).into_response())
}

async fn rename_or_chmod(
    State(svc): State<Arc<FilesService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, path)): Path<(Uuid, String)>,
    body: Bytes,
) -> ApiResult<Response> {
    let req: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let from = FilePath::new(path).map_err(map_file_err)?;
    if let Some(to_str) = req.get("to").and_then(|v| v.as_str()) {
        let to = FilePath::new(to_str.to_string()).map_err(map_file_err)?;
        svc.rename(&user, site_id, &from, &to)
            .await
            .map_err(map_file_err)?;
        return Ok(Json(json!({"ok": true, "op": "rename"})).into_response());
    }
    if let Some(mode_str) = req.get("mode").and_then(|v| v.as_str()) {
        let mode = u32::from_str_radix(mode_str.trim_start_matches('0'), 8)
            .map_err(|e| ApiError::BadRequest(format!("invalid octal `{}`: {e}", mode_str)))?;
        svc.chmod(&user, site_id, &from, mode)
            .await
            .map_err(map_file_err)?;
        return Ok(Json(json!({"ok": true, "op": "chmod"})).into_response());
    }
    Err(ApiError::BadRequest(
        "PATCH body must contain `to` (rename) or `mode` (chmod)".into(),
    ))
}

fn map_file_err(e: FileError) -> ApiError {
    match e {
        FileError::Forbidden => ApiError::Forbidden,
        FileError::SiteNotFound(_) => ApiError::NotFound(e.to_string()),
        FileError::NotFound(_) => ApiError::NotFound(e.to_string()),
        FileError::InvalidPath(_) | FileError::PathOutsideChroot(_) => {
            ApiError::BadRequest(e.to_string())
        }
        FileError::AlreadyExists(_) => ApiError::Conflict(e.to_string()),
        FileError::FileTooLarge(size) => ApiError::PayloadTooLarge(size),
        FileError::IsADirectory(_)
        | FileError::IsNotADirectory(_)
        | FileError::DirectoryNotEmpty(_)
        | FileError::Io(_) => ApiError::Internal(e.to_string()),
    }
}
