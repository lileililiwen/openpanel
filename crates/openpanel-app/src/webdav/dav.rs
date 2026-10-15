//! Axum WebDAV router mounted under `/dav`.
//!
//! Path scheme: `/dav/{account_id}/{rel path}`. Requests authenticate
//! with HTTP Basic; the username must equal the account's username and
//! the password is verified against the stored Argon2 hash. Every path
//! is jailed: traversal, absolute escapes, and symlink escapes render
//! as `404 Not Found` so the endpoint leaks nothing about the host
//! filesystem. Read-only accounts reject every mutating verb with 403.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path as AxPath, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::Response,
    routing::any,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use futures_util::future::{BoxFuture, FutureExt};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use super::service::WebDavService;
use openpanel_domain::webdav::{WebDavAccount, WebDavError};

/// Maximum request body the DAV endpoint accepts (64 MiB).
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

const ALLOW_CSV: &str = "OPTIONS, PROPFIND, HEAD, GET, PUT, DELETE, MKCOL, MOVE, COPY, PROPPATCH";

/// Shared state for the DAV router.
#[derive(Clone)]
struct DavState {
    service: Arc<WebDavService>,
}

/// Build the `/dav` subtree router (mounted outside session auth by
/// the API composition root).
#[must_use]
pub fn router(service: Arc<WebDavService>) -> Router {
    Router::new()
        .route("/", any(any_dav_root))
        .route("/{account_id}", any(any_dav_account))
        .route("/{account_id}/{*path}", any(any_dav_scoped))
        .with_state(DavState { service })
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

async fn any_dav_root(method: Method) -> Response<Body> {
    // `/dav` carries no account id; everything is 404 except OPTIONS.
    if method == Method::OPTIONS {
        return options_response();
    }
    not_found()
}

async fn any_dav_account(
    State(state): State<DavState>,
    method: Method,
    AxPath(account_id): AxPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    dispatch(state, method, &account_id, "", &headers, body).await
}

async fn any_dav_scoped(
    State(state): State<DavState>,
    method: Method,
    AxPath((account_id, rel)): AxPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    dispatch(state, method, &account_id, &rel, &headers, body).await
}

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    state: DavState,
    method: Method,
    raw_account_id: &str,
    rel: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Response<Body> {
    if method == Method::OPTIONS {
        return options_response();
    }
    let account = match authenticate(&state, raw_account_id, headers).await {
        Ok(account) => account,
        Err(response) => return *response,
    };
    let target = match jail_resolve(&account, rel).await {
        Ok(target) => target,
        Err(_) => return not_found(),
    };
    if is_mutating(&method) && account.read_only() {
        return simple(StatusCode::FORBIDDEN, "account is read-only");
    }
    match method.as_str() {
        "GET" => read_resource(&target, false).await,
        "HEAD" => read_resource(&target, true).await,
        "PUT" => write_resource(&target, body).await,
        "DELETE" => delete_resource(&target).await,
        "MKCOL" => mkcol(&target).await,
        "MOVE" => move_resource(&account, raw_account_id, &target, headers).await,
        "COPY" => copy_resource(&account, raw_account_id, &target, headers).await,
        "PROPFIND" => propfind(&account, raw_account_id, rel, &target, headers).await,
        "PROPPATCH" => proppatch(raw_account_id, rel),
        "LOCK" | "UNLOCK" | "ACL" => {
            simple(StatusCode::NOT_IMPLEMENTED, "locking is not supported")
        }
        _ => method_not_allowed(),
    }
}

fn is_mutating(method: &Method) -> bool {
    matches!(
        method.as_str(),
        "PUT" | "DELETE" | "MKCOL" | "MOVE" | "COPY" | "PROPPATCH"
    )
}

async fn authenticate(
    state: &DavState,
    raw_account_id: &str,
    headers: &HeaderMap,
) -> Result<WebDavAccount, Box<Response<Body>>> {
    let Ok(account_id) = Uuid::parse_str(raw_account_id) else {
        // A malformed account segment must not disclose whether the
        // route exists at all.
        return Err(Box::new(not_found()));
    };
    let Some((username, password)) = basic_credentials(headers) else {
        return Err(Box::new(challenge()));
    };
    let ip = peer_ip(headers);
    match state
        .service
        .authenticate(account_id, username.as_str(), password.as_str(), &ip)
        .await
    {
        Ok(account) => Ok(account),
        Err(WebDavError::LoginDenied) => Err(Box::new(challenge())),
        Err(_) => Err(Box::new(simple(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal error",
        ))),
    }
}

/// Best-effort client identity for the audit trail: the leftmost
/// `X-Forwarded-For` entry when behind a proxy, `Forwarded for=`
/// otherwise, `"unknown"` when neither is present.
fn peer_ip(headers: &HeaderMap) -> String {
    if let Some(value) = header_value(headers, "x-forwarded-for")
        && let Some(first) = value.split(',').next().map(str::trim)
        && !first.is_empty()
    {
        return first.to_owned();
    }
    if let Some(value) = header_value(headers, "forwarded")
        && let Some(for_part) = value
            .split(';')
            .find_map(|token| token.trim().strip_prefix("for="))
    {
        return for_part
            .trim_matches(|c: char| c == '"' || c == '[' || c == ']')
            .to_owned();
    }
    "unknown".to_owned()
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let encoded = raw.strip_prefix("Basic ")?.trim();
    let decoded = STANDARD.decode(encoded).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (username, password) = text.split_once(':')?;
    Some((username.to_owned(), password.to_owned()))
}

async fn jail_resolve(account: &WebDavAccount, rel: &str) -> Result<PathBuf, WebDavError> {
    let joined = account.resolve(Path::new(rel))?;
    let home_real = tokio::fs::canonicalize(account.home())
        .await
        .map_err(|_| WebDavError::OutsideJail)?;
    if let Ok(real) = tokio::fs::canonicalize(&joined).await {
        if real.starts_with(&home_real) {
            return Ok(joined);
        }
        return Err(WebDavError::OutsideJail);
    }
    let mut cursor = joined.clone();
    loop {
        if let Ok(real) = tokio::fs::canonicalize(&cursor).await {
            return if real.starts_with(&home_real) {
                Ok(joined)
            } else {
                Err(WebDavError::OutsideJail)
            };
        }
        if !cursor.pop() {
            return Err(WebDavError::OutsideJail);
        }
    }
}

async fn read_resource(target: &Path, head_only: bool) -> Response<Body> {
    let Ok(meta) = tokio::fs::metadata(target).await else {
        return not_found();
    };
    if meta.is_dir() {
        return simple(StatusCode::METHOD_NOT_ALLOWED, "resource is a collection");
    }
    let Ok(mut file) = tokio::fs::File::open(target).await else {
        return not_found();
    };
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).await.is_err() {
        return server_error();
    }
    let length = buf.len();
    let mut response = Response::new(Body::from(if head_only { Vec::new() } else { buf }));
    {
        let headers = response.headers_mut();
        if let Ok(value) = HeaderValue::from_str(&content_type(target)) {
            headers.insert(axum::http::header::CONTENT_TYPE, value);
        }
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            HeaderValue::from_str(&length.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("0")),
        );
        if let Some(value) = http_date(system_time(&meta)) {
            headers.insert(axum::http::header::LAST_MODIFIED, value);
        }
    }
    *response.status_mut() = StatusCode::OK;
    response
}

async fn write_resource(target: &Path, body: Bytes) -> Response<Body> {
    let existed = tokio::fs::symlink_metadata(target).await.is_ok();
    if !parent_exists(target).await {
        return simple(
            StatusCode::CONFLICT,
            "intermediate collection does not exist",
        );
    }
    if tokio::fs::write(target, body).await.is_err() {
        return server_error();
    }
    simple(
        if existed {
            StatusCode::NO_CONTENT
        } else {
            StatusCode::CREATED
        },
        "",
    )
}

async fn delete_resource(target: &Path) -> Response<Body> {
    let Ok(meta) = tokio::fs::symlink_metadata(target).await else {
        return not_found();
    };
    let removed = if meta.is_dir() && !meta.file_type().is_symlink() {
        tokio::fs::remove_dir_all(target).await
    } else {
        tokio::fs::remove_file(target).await
    };
    if removed.is_err() {
        return server_error();
    }
    simple(StatusCode::NO_CONTENT, "")
}

async fn mkcol(target: &Path) -> Response<Body> {
    if tokio::fs::symlink_metadata(target).await.is_ok() {
        return simple(StatusCode::METHOD_NOT_ALLOWED, "collection already exists");
    }
    if !parent_exists(target).await {
        return simple(
            StatusCode::CONFLICT,
            "intermediate collection does not exist",
        );
    }
    if tokio::fs::create_dir(target).await.is_err() {
        return server_error();
    }
    simple(StatusCode::CREATED, "")
}

async fn move_resource(
    account: &WebDavAccount,
    raw_account_id: &str,
    target: &Path,
    headers: &HeaderMap,
) -> Response<Body> {
    if tokio::fs::symlink_metadata(target).await.is_err() {
        return not_found();
    }
    let Some(dest_path) = destination_path(raw_account_id, headers) else {
        return simple(StatusCode::BAD_REQUEST, "invalid Destination header");
    };
    let dest = match jail_resolve(account, &dest_path).await {
        Ok(dest) => dest,
        Err(_) => return not_found(),
    };
    if dest == target {
        return simple(StatusCode::BAD_REQUEST, "source and destination are equal");
    }
    let dest_existed = tokio::fs::symlink_metadata(&dest).await.is_ok();
    if dest_existed && overwrite_denied(headers) {
        return simple(StatusCode::PRECONDITION_FAILED, "destination exists");
    }
    if !parent_exists(&dest).await {
        return simple(
            StatusCode::CONFLICT,
            "intermediate collection does not exist",
        );
    }
    if dest_existed {
        let Ok(meta) = tokio::fs::symlink_metadata(&dest).await else {
            return server_error();
        };
        let cleared = if meta.is_dir() && !meta.file_type().is_symlink() {
            tokio::fs::remove_dir_all(&dest).await
        } else {
            tokio::fs::remove_file(&dest).await
        };
        if cleared.is_err() {
            return server_error();
        }
    }
    if tokio::fs::rename(target, &dest).await.is_err() {
        return server_error();
    }
    simple(
        if dest_existed {
            StatusCode::NO_CONTENT
        } else {
            StatusCode::CREATED
        },
        "",
    )
}

async fn copy_resource(
    account: &WebDavAccount,
    raw_account_id: &str,
    target: &Path,
    headers: &HeaderMap,
) -> Response<Body> {
    if tokio::fs::symlink_metadata(target).await.is_err() {
        return not_found();
    }
    let Some(dest_path) = destination_path(raw_account_id, headers) else {
        return simple(StatusCode::BAD_REQUEST, "invalid Destination header");
    };
    let dest = match jail_resolve(account, &dest_path).await {
        Ok(dest) => dest,
        Err(_) => return not_found(),
    };
    if dest == target {
        return simple(StatusCode::BAD_REQUEST, "source and destination are equal");
    }
    let dest_existed = tokio::fs::symlink_metadata(&dest).await.is_ok();
    if dest_existed && overwrite_denied(headers) {
        return simple(StatusCode::PRECONDITION_FAILED, "destination exists");
    }
    if !parent_exists(&dest).await {
        return simple(
            StatusCode::CONFLICT,
            "intermediate collection does not exist",
        );
    }
    if copy_tree(target, &dest).await.is_err() {
        return server_error();
    }
    simple(
        if dest_existed {
            StatusCode::NO_CONTENT
        } else {
            StatusCode::CREATED
        },
        "",
    )
}

fn copy_tree<'a>(from: &'a Path, to: &'a Path) -> BoxFuture<'a, std::io::Result<()>> {
    async move {
        let meta = tokio::fs::symlink_metadata(from).await?;
        if meta.is_dir() && !meta.file_type().is_symlink() {
            tokio::fs::create_dir_all(to).await?;
            let mut entries = tokio::fs::read_dir(from).await?;
            while let Some(entry) = entries.next_entry().await? {
                copy_tree(&entry.path(), &to.join(entry.file_name())).await?;
            }
            Ok(())
        } else {
            tokio::fs::copy(from, to).await.map(|_| ())
        }
    }
    .boxed()
}

fn destination_path(raw_account_id: &str, headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("destination")?.to_str().ok()?.trim().to_owned();
    let path = match raw.split_once("://") {
        Some((_scheme, rest)) => rest.split_once('/')?.1.to_owned(),
        None => raw,
    };
    let prefix = format!("/dav/{raw_account_id}");
    let tail = path.strip_prefix(&prefix)?;
    Some(percent_decode(tail.trim_start_matches('/')))
}

fn overwrite_denied(headers: &HeaderMap) -> bool {
    headers
        .get("overwrite")
        .and_then(|value| value.to_str().ok())
        == Some("F")
}

async fn parent_exists(target: &Path) -> bool {
    match target.parent() {
        Some(parent) => tokio::fs::metadata(parent).await.is_ok(),
        None => false,
    }
}

async fn propfind(
    _account: &WebDavAccount,
    raw_account_id: &str,
    rel: &str,
    target: &Path,
    headers: &HeaderMap,
) -> Response<Body> {
    let Ok(meta) = tokio::fs::symlink_metadata(target).await else {
        return not_found();
    };
    let depth = if header_value(headers, "depth").unwrap_or("1") == "0" {
        0
    } else {
        1
    };
    let root_href = dav_href(raw_account_id, rel, meta.is_dir());
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:multistatus xmlns:D=\"DAV:\">\n",
    );
    push_response(&mut xml, &root_href, target, &meta);
    if meta.is_dir()
        && depth == 1
        && let Ok(mut entries) = tokio::fs::read_dir(target).await
    {
        let mut names = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            names.push(entry);
        }
        names.sort_by_key(|entry| entry.file_name());
        for entry in names {
            if let Ok(child_meta) = entry.metadata().await {
                let child_rel = if rel.is_empty() {
                    entry.file_name().to_string_lossy().into_owned()
                } else {
                    format!("{rel}/{}", entry.file_name().to_string_lossy())
                };
                let href = dav_href(raw_account_id, &child_rel, child_meta.is_dir());
                push_response(&mut xml, &href, &entry.path(), &child_meta);
            }
        }
    }
    xml.push_str("</D:multistatus>\n");
    xml_response(StatusCode::MULTI_STATUS, xml)
}

fn push_response(xml: &mut String, href: &str, path: &Path, meta: &std::fs::Metadata) {
    xml.push_str("<D:response>\n<D:href>");
    xml.push_str(&xml_escape(href));
    xml.push_str("</D:href>\n<D:propstat>\n<D:prop>\n");
    xml.push_str(&format!(
        "<D:displayname>{}</D:displayname>\n",
        xml_escape(
            &path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        )
    ));
    if meta.is_dir() {
        xml.push_str("<D:resourcetype><D:collection/></D:resourcetype>\n");
    } else {
        xml.push_str("<D:resourcetype/>\n");
        xml.push_str(&format!(
            "<D:getcontentlength>{}</D:getcontentlength>\n",
            meta.len()
        ));
    }
    if let Some(value) = http_date(system_time(meta))
        && let Ok(text) = value.to_str()
    {
        xml.push_str(&format!(
            "<D:getlastmodified>{}</D:getlastmodified>\n",
            xml_escape(text)
        ));
    }
    xml.push_str("</D:prop>\n<D:status>HTTP/1.1 200 OK</D:status>\n</D:propstat>\n</D:response>\n");
}

fn proppatch(raw_account_id: &str, rel: &str) -> Response<Body> {
    let href = dav_href(raw_account_id, rel, true);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:multistatus xmlns:D=\"DAV:\">\n<D:response>\n<D:href>{}</D:href>\n<D:propstat>\n<D:prop></D:prop>\n<D:status>HTTP/1.1 200 OK</D:status>\n</D:propstat>\n</D:response>\n</D:multistatus>\n",
        xml_escape(&href)
    );
    xml_response(StatusCode::MULTI_STATUS, xml)
}

fn options_response() -> Response<Body> {
    let mut response = Response::new(Body::empty());
    {
        let headers = response.headers_mut();
        headers.insert("dav", HeaderValue::from_static("1"));
        if let Ok(value) = HeaderValue::from_str(ALLOW_CSV) {
            headers.insert(axum::http::header::ALLOW, value);
        }
        headers.insert("ms-author-via", HeaderValue::from_static("DAV"));
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            HeaderValue::from_static("0"),
        );
    }
    *response.status_mut() = StatusCode::OK;
    response
}

fn method_not_allowed() -> Response<Body> {
    let mut response = simple(StatusCode::METHOD_NOT_ALLOWED, "method not allowed");
    if let Ok(value) = HeaderValue::from_str(ALLOW_CSV) {
        response
            .headers_mut()
            .insert(axum::http::header::ALLOW, value);
    }
    response
}

fn challenge() -> Response<Body> {
    let mut response = simple(StatusCode::UNAUTHORIZED, "authentication required");
    response.headers_mut().insert(
        axum::http::header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Basic realm=\"WebDAV\""),
    );
    response
}

fn not_found() -> Response<Body> {
    simple(StatusCode::NOT_FOUND, "resource not found")
}

fn server_error() -> Response<Body> {
    simple(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}

fn simple(status: StatusCode, message: &str) -> Response<Body> {
    let mut response = Response::new(Body::from(message.to_owned()));
    {
        let headers = response.headers_mut();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
    }
    *response.status_mut() = status;
    response
}

fn xml_response(status: StatusCode, body: String) -> Response<Body> {
    let mut response = Response::new(Body::from(body));
    {
        let headers = response.headers_mut();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/xml; charset=utf-8"),
        );
    }
    *response.status_mut() = status;
    response
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn percent_encode_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(*byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2]))
        {
            out.push(hi * 16 + lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn dav_href(raw_account_id: &str, rel: &str, is_dir: bool) -> String {
    let mut href = format!("/dav/{raw_account_id}");
    for segment in rel.split('/').filter(|segment| !segment.is_empty()) {
        href.push('/');
        href.push_str(&percent_encode_segment(segment));
    }
    if is_dir {
        href.push('/');
    }
    href
}

fn system_time(meta: &std::fs::Metadata) -> Option<std::time::SystemTime> {
    meta.modified().ok().or_else(|| meta.created().ok())
}

fn http_date(time: Option<std::time::SystemTime>) -> Option<HeaderValue> {
    let stamp = DateTime::<Utc>::from(time?).to_rfc2822();
    let text = stamp.replace(" +0000", " GMT");
    HeaderValue::from_str(&text).ok()
}

fn content_type(path: &Path) -> String {
    let name = path
        .file_name()
        .map_or_else(String::new, |value| value.to_string_lossy().into_owned())
        .to_ascii_lowercase();
    [
        (".html", "text/html; charset=utf-8"),
        (".htm", "text/html; charset=utf-8"),
        (".css", "text/css; charset=utf-8"),
        (".js", "application/javascript"),
        (".mjs", "application/javascript"),
        (".json", "application/json"),
        (".xml", "application/xml"),
        (".txt", "text/plain; charset=utf-8"),
        (".php", "text/plain; charset=utf-8"),
        (".png", "image/png"),
        (".jpg", "image/jpeg"),
        (".jpeg", "image/jpeg"),
        (".gif", "image/gif"),
        (".svg", "image/svg+xml"),
        (".webp", "image/webp"),
        (".ico", "image/x-icon"),
        (".pdf", "application/pdf"),
        (".zip", "application/zip"),
        (".gz", "application/gzip"),
        (".woff", "font/woff"),
        (".woff2", "font/woff2"),
    ]
    .iter()
    .find(|(suffix, _)| name.ends_with(suffix))
    .map_or_else(
        || "application/octet-stream".to_owned(),
        |(_, value)| (*value).to_owned(),
    )
}
