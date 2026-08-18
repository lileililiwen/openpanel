//! File manager pages: per-site chrooted directory listing, read/write
//! editor, mkdir / rename / chmod / remove, and multipart upload — all
//! rendered inside the shell, all routed through the existing
//! `FilesService`.
//!
//! The chroot boundary is enforced in exactly one place (the service).
//! The web layer hands every caller-supplied path to the service
//! verbatim and renders the service's verdict (allowed, `..` rejected,
//! not found, ...). No path manipulation happens here beyond
//! URL-encoding the segments that go into `href`s.

use axum::{
    extract::{Multipart, Path as AxPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use maud::{Markup, html};
use openpanel_domain::{
    User,
    files::{file_info::FileInfo, path::Path as FsPath},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// One row in the file listing, pre-resolved for rendering.
pub struct FileRow {
    /// Basename of the entry.
    pub name: String,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Human-readable size (`1.4 KB` for files, `—` for dirs).
    pub size: String,
    /// Octal mode string.
    pub mode: String,
    /// Last-modified timestamp (formatted).
    pub mtime: String,
    /// MIME type, when known.
    pub mime: Option<String>,
}

/// Query parameters for `GET /sites/{id}/files` and the per-row actions.
#[derive(Debug, Deserialize)]
pub struct FilesQuery {
    /// Site-relative path (empty = root).
    #[serde(default)]
    pub path: Option<String>,
    /// CSRF token (used for the `DELETE /files/remove` flow where the
    /// body is the form payload and CSRF can ride along in the query).
    #[serde(default)]
    pub csrf: String,
}

/// Body of the write form.
#[derive(Debug, Deserialize)]
pub struct WriteForm {
    /// Site-relative path to write.
    #[serde(default)]
    pub path: String,
    /// Plain-text file contents.
    #[serde(default)]
    pub contents: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the mkdir form.
#[derive(Debug, Deserialize)]
pub struct MkdirForm {
    /// Site-relative path of the new directory.
    #[serde(default)]
    pub path: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the rename form.
#[derive(Debug, Deserialize)]
pub struct RenameForm {
    /// Source path.
    #[serde(default)]
    pub from: String,
    /// Destination path.
    #[serde(default)]
    pub to: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the chmod form.
#[derive(Debug, Deserialize)]
pub struct ChmodForm {
    /// Site-relative path.
    #[serde(default)]
    pub path: String,
    /// Octal mode bits (e.g. `644`).
    #[serde(default)]
    pub mode: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// `GET /files` — choose a visible site before entering its chrooted manager.
pub async fn landing(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let sites = state.sites.list_sites(&user).await.unwrap_or_default();
    let content = html! {
        h1 { "Files" }
        p { "Choose a site to manage its document root." }
        @if sites.is_empty() {
            (crate::ui_states::EmptyState::new("No sites available", "Create a site before managing its files.").render())
        } @else {
            ul class="resource-links" {
                @for site in sites {
                    li {
                        a href=(format!("/sites/{}/files", site.id())) {
                            (site.primary_domain())
                        }
                    }
                }
            }
        }
    };
    state
        .render_shell(&user, &csrf, "/files", content)
        .await
        .into_response()
}

/// `GET /sites/{site_id}/files?path=` — the directory listing.
pub async fn list(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Query(q): Query<FilesQuery>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let raw_path = q.path.unwrap_or_default();
    let result = render_listing(&state, &user, site_id, &raw_path).await;
    match result {
        ListingOutcome::Ok {
            rows,
            current,
            parent,
            error,
        } => {
            let content = html! {
                h1 { "Files" }
                (breadcrumb(site_id, &current, &parent))
                @if let Some(msg) = error {
                    (error_region(&msg))
                }
                (listing_fragment(site_id, &current, &rows, &csrf))
            };
            state
                .render_shell(&user, &csrf, "/files", content)
                .await
                .into_response()
        }
        ListingOutcome::Forbidden => (StatusCode::FORBIDDEN, "forbidden").into_response(),
        ListingOutcome::NotFound => (StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

/// `GET /sites/{site_id}/files/read?path=` — file contents (text) or
/// download notice (binary/large).
pub async fn read(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Query(q): Query<FilesQuery>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let raw_path = q.path.unwrap_or_default();
    let path = match FsPath::new(&raw_path) {
        Ok(p) => p,
        Err(e) => {
            return render_read_error(&state, &user, &csrf, site_id, &e.to_string()).await;
        }
    };
    match state.files.read_file(&user, site_id, &path).await {
        Ok((bytes, mtime)) => {
            let is_text = is_probably_text(&bytes);
            let content = if is_text && bytes.len() <= 50 * 1024 * 1024 {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                html! {
                    h1 { "Edit file" }
                    p { code { (raw_path) } }
                    (editor_panel(site_id, &raw_path, &text, mtime, &csrf))
                }
            } else {
                html! {
                    h1 { (raw_path) }
                    p class="alert" { "Binary or large file — download only." }
                }
            };
            state
                .render_shell(&user, &csrf, "/files", content)
                .await
                .into_response()
        }
        Err(e) => render_read_error(&state, &user, &csrf, site_id, &e.to_string()).await,
    }
}

/// `POST /sites/{site_id}/files/write` — save text contents.
pub async fn write(
    State(state): State<WebState>,
    WebUser(user, _session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<WriteForm>,
) -> Response {
    if !state.csrf.verify(_session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let path = match FsPath::new(&form.path) {
        Ok(p) => p,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let result = state
        .files
        .write_file(&user, site_id, &path, form.contents.as_bytes())
        .await;
    let csrf = state.csrf.token_for(_session.id());
    action_response(&state, &user, site_id, "", &csrf, result.map(|_| ())).await
}

/// `POST /sites/{site_id}/files/mkdir` — create a directory.
pub async fn mkdir(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<MkdirForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let path = match FsPath::new(&form.path) {
        Ok(p) => p,
        Err(e) => {
            return action_response(
                &state,
                &user,
                site_id,
                "",
                &csrf,
                Err(openpanel_domain::files::error::FileError::InvalidPath(
                    e.to_string(),
                )),
            )
            .await;
        }
    };
    let result = state.files.mkdir(&user, site_id, &path).await;
    action_response(&state, &user, site_id, &form.path, &csrf, result).await
}

/// `POST /sites/{site_id}/files/rename` — rename / move a file or dir.
pub async fn rename(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<RenameForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let from = match FsPath::new(&form.from) {
        Ok(p) => p,
        Err(e) => {
            return action_response(
                &state,
                &user,
                site_id,
                "",
                &csrf,
                Err(openpanel_domain::files::error::FileError::InvalidPath(
                    e.to_string(),
                )),
            )
            .await;
        }
    };
    let to = match FsPath::new(&form.to) {
        Ok(p) => p,
        Err(e) => {
            return action_response(
                &state,
                &user,
                site_id,
                "",
                &csrf,
                Err(openpanel_domain::files::error::FileError::InvalidPath(
                    e.to_string(),
                )),
            )
            .await;
        }
    };
    let result = state.files.rename(&user, site_id, &from, &to).await;
    action_response(&state, &user, site_id, &form.to, &csrf, result).await
}

/// `POST /sites/{site_id}/files/chmod` — change mode bits.
pub async fn chmod(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<ChmodForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let path = match FsPath::new(&form.path) {
        Ok(p) => p,
        Err(e) => {
            return action_response(
                &state,
                &user,
                site_id,
                "",
                &csrf,
                Err(openpanel_domain::files::error::FileError::InvalidPath(
                    e.to_string(),
                )),
            )
            .await;
        }
    };
    let mode = match u32::from_str_radix(form.mode.trim().trim_start_matches('0'), 8) {
        Ok(m) => m,
        Err(_) => {
            return action_response(
                &state,
                &user,
                site_id,
                "",
                &csrf,
                Err(openpanel_domain::files::error::FileError::InvalidPath(
                    "mode must be octal".into(),
                )),
            )
            .await;
        }
    };
    let result = state.files.chmod(&user, site_id, &path, mode).await;
    action_response(&state, &user, site_id, &form.path, &csrf, result).await
}

/// Body of the remove form.
#[derive(Debug, Deserialize)]
pub struct RemoveForm {
    /// Site-relative path to delete.
    #[serde(default)]
    pub path: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// `DELETE /sites/{site_id}/files/remove` — delete (recursive).
pub async fn remove(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<RemoveForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let raw = form.path.clone();
    let path = match FsPath::new(&raw) {
        Ok(p) => p,
        Err(e) => {
            return action_response(
                &state,
                &user,
                site_id,
                "",
                &csrf,
                Err(openpanel_domain::files::error::FileError::InvalidPath(
                    e.to_string(),
                )),
            )
            .await;
        }
    };
    let result = state.files.remove(&user, site_id, &path, true).await;
    action_response(&state, &user, site_id, &raw, &csrf, result).await
}

/// `POST /sites/{site_id}/files/upload` — multipart upload.
pub async fn upload(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    mut multipart: Multipart,
) -> Response {
    let csrf = state.csrf.token_for(session.id());

    let mut target_dir = String::new();
    let mut csrf_token = String::new();
    let mut filename = String::new();
    let mut bytes: Vec<u8> = Vec::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(_) => break,
        };
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                filename = field.file_name().unwrap_or("upload").to_string();
                let mut field = field;
                while let Some(chunk) = field.chunk().await.transpose() {
                    match chunk {
                        Ok(c) => bytes.extend_from_slice(&c),
                        Err(_) => break,
                    }
                }
            }
            "path" => {
                if let Ok(text) = field.text().await {
                    target_dir = text;
                }
            }
            "_csrf" => {
                if let Ok(text) = field.text().await {
                    csrf_token = text;
                }
            }
            _ => {}
        }
    }
    if !state.csrf.verify(session.id(), &csrf_token) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let target = match FsPath::new(&target_dir) {
        Ok(p) => p,
        Err(e) => {
            return action_response(
                &state,
                &user,
                site_id,
                "",
                &csrf,
                Err(openpanel_domain::files::error::FileError::InvalidPath(
                    e.to_string(),
                )),
            )
            .await;
        }
    };
    let joined = match target.join(&filename) {
        Ok(p) => p,
        Err(e) => {
            return action_response(
                &state,
                &user,
                site_id,
                "",
                &csrf,
                Err(openpanel_domain::files::error::FileError::InvalidPath(
                    e.to_string(),
                )),
            )
            .await;
        }
    };
    let result = state
        .files
        .write_file(&user, site_id, &joined, &bytes)
        .await;
    action_response(&state, &user, site_id, &target_dir, &csrf, result).await
}

/// Outcome of resolving a directory listing.
enum ListingOutcome {
    /// Listing rendered with rows, current path, parent, optional error.
    Ok {
        rows: Vec<FileRow>,
        current: String,
        parent: Option<String>,
        error: Option<String>,
    },
    /// Caller lacks permission for the site.
    Forbidden,
    /// Site does not exist.
    NotFound,
}

async fn render_listing(
    state: &WebState,
    user: &User,
    site_id: Uuid,
    raw_path: &str,
) -> ListingOutcome {
    let path = match FsPath::new(raw_path) {
        Ok(p) => p,
        Err(e) => {
            return ListingOutcome::Ok {
                rows: Vec::new(),
                current: raw_path.to_string(),
                parent: None,
                error: Some(e.to_string()),
            };
        }
    };
    match state.files.list_dir(user, site_id, &path).await {
        Ok(entries) => ListingOutcome::Ok {
            rows: collect_rows(&entries),
            current: raw_path.to_string(),
            parent: parent_of(raw_path),
            error: None,
        },
        Err(openpanel_domain::files::error::FileError::Forbidden) => ListingOutcome::Forbidden,
        Err(openpanel_domain::files::error::FileError::SiteNotFound(_)) => ListingOutcome::NotFound,
        Err(e) => ListingOutcome::Ok {
            rows: Vec::new(),
            current: raw_path.to_string(),
            parent: None,
            error: Some(e.to_string()),
        },
    }
}

/// Run a state-changing action and re-render the listing fragment.
async fn action_response<E: std::fmt::Display>(
    state: &WebState,
    user: &User,
    site_id: Uuid,
    current_path: &str,
    csrf: &str,
    result: Result<(), E>,
) -> Response {
    let error = result.err().map(|e| e.to_string());
    let outcome = render_listing(state, user, site_id, current_path).await;
    match outcome {
        ListingOutcome::Ok {
            rows,
            current,
            error: list_err,
            ..
        } => {
            let fragment = match &error {
                Some(msg) => html! {
                    (error_region(msg))
                    (listing_fragment(site_id, &current, &rows, csrf))
                },
                None => match &list_err {
                    Some(msg) => html! {
                        (error_region(msg))
                        (listing_fragment(site_id, &current, &rows, csrf))
                    },
                    None => listing_fragment(site_id, &current, &rows, csrf),
                },
            };
            (StatusCode::OK, fragment).into_response()
        }
        _ => (StatusCode::OK, error_region("unexpected listing error")).into_response(),
    }
}

async fn render_read_error(
    state: &WebState,
    user: &User,
    csrf: &str,
    site_id: Uuid,
    msg: &str,
) -> Response {
    let _ = site_id;
    let content = html! {
        h1 { "Read file" }
        (error_region(msg))
        p { a href=(format!("/sites/{site_id}/files")) { "Back to listing" } }
    };
    state
        .render_shell(user, csrf, "/files", content)
        .await
        .into_response()
}

/// Compute the parent of a site-relative path, or `None` at the root.
fn parent_of(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    match path.rfind('/') {
        Some(idx) => Some(path[..idx].to_string()),
        None => Some(String::new()),
    }
}

/// URL-encode a single path segment. Segments are encoded in isolation
/// so `/` is the only character treated as a separator. The encoder is
/// hand-rolled to avoid pulling in a new dependency.
fn encode_segment(seg: &str) -> String {
    let mut out = String::with_capacity(seg.len());
    for byte in seg.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    out
}

/// Encode a full site-relative path (preserve `/` separators).
fn encode_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    path.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// Render the breadcrumb (site root → … → current).
fn breadcrumb(site_id: Uuid, current: &str, parent: &Option<String>) -> Markup {
    let parts: Vec<&str> = if current.is_empty() {
        Vec::new()
    } else {
        current.split('/').collect()
    };
    let mut prefixes: Vec<String> = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            prefixes.push(part.to_string());
        } else {
            let mut prev = prefixes[i - 1].clone();
            prev.push('/');
            prev.push_str(part);
            prefixes.push(prev);
        }
    }
    html! {
        nav class="breadcrumb" {
            a href=(format!("/sites/{site_id}/files")) { "root" }
            @if !parts.is_empty() {
                " / "
                @for (i, part) in parts.iter().enumerate() {
                    @if i > 0 { " / " }
                    @if i + 1 == parts.len() {
                        strong { (part) }
                    } @else {
                        a href=(format!("/sites/{site_id}/files?path={}", encode_path(&prefixes[i]))) { (part) }
                    }
                }
            }
            @if let Some(p) = parent {
                @if !p.is_empty() {
                    " — "
                    a href=(format!("/sites/{site_id}/files?path={}", encode_path(p))) { "↑ up" }
                }
            }
        }
    }
}

/// Render the `#files-listing` fragment: a table of entries with
/// per-row actions (read, rename, chmod, delete) and a footer with
/// the create/upload forms.
pub fn listing_fragment(site_id: Uuid, current: &str, rows: &[FileRow], csrf: &str) -> Markup {
    html! {
        section id="files-listing" {
            @if rows.is_empty() {
                (crate::ui_states::EmptyState::new("Empty directory", "This directory has no files yet. Upload or create one below.").render())
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Name" }
                            th { "Type" }
                            th { "Size" }
                            th { "Mode" }
                            th { "Modified" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for row in rows {
                            tr {
                                td { a href=(href_entry(site_id, current, &row.name, row.is_dir)) { (row.name) } }
                                td { @if row.is_dir { "dir" } @else { "file" } }
                                td { (row.size) }
                                td { code { (row.mode) } }
                                td { (row.mtime) }
                                td class="actions" {
                                    @if !row.is_dir {
                                        a href=(href_read(site_id, current, &row.name)) { "Read" }
                                    }
                                    form class="inline" hx-post=(href_rename(site_id)) hx-target="#files-listing" {
                                        (csrf_field(csrf))
                                        input type="hidden" name="from" value=(full_path(current, &row.name));
                                        input type="text" name="to" placeholder="rename to" size="14";
                                        button type="submit" { "Rename" }
                                    }
                                    form class="inline" hx-post=(href_chmod(site_id)) hx-target="#files-listing" {
                                        (csrf_field(csrf))
                                        input type="hidden" name="path" value=(full_path(current, &row.name));
                                        input type="text" name="mode" value=(row.mode) size="3";
                                        button type="submit" { "chmod" }
                                    }
                                    a class="btn danger" hx-get=(format!("/layer/confirm?action=delete-entry&site_id={site_id}&path={}", crate::layer::urlencode(&full_path(current, &row.name))))
                                        hx-target="#layer-root" href=(format!("/layer/confirm?action=delete-entry&site_id={site_id}&path={}", crate::layer::urlencode(&full_path(current, &row.name)))) {
                                        "Delete"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            footer class="actions" {
                form class="inline" hx-post=(href_mkdir(site_id)) hx-target="#files-listing" {
                    (csrf_field(csrf))
                    input type="hidden" name="path" value=(current);
                    input type="text" name="path" placeholder="new directory" size="20";
                    button type="submit" { "mkdir" }
                }
                form class="inline" hx-post=(href_upload(site_id)) hx-encoding="multipart/form-data" hx-target="#files-listing" {
                    (csrf_field(csrf))
                    input type="hidden" name="path" value=(current);
                    input type="file" name="file";
                    button type="submit" { "Upload" }
                }
            }
        }
    }
}

/// Render the read/edit panel.
pub fn editor_panel(
    site_id: Uuid,
    path: &str,
    contents: &str,
    _mtime: DateTime<Utc>,
    csrf: &str,
) -> Markup {
    html! {
        form method="post" action=(href_write(site_id)) class="form" {
            (csrf_field(csrf))
            input type="hidden" name="path" value=(path);
            textarea name="contents" rows="20" cols="80" { (contents) }
            button type="submit" { "Save" }
        }
    }
}

/// Render an inline error region.
pub fn error_region(message: &str) -> Markup {
    html! {
        div class="alert error" role="alert" { (message) }
    }
}

fn href_entry(site_id: Uuid, current: &str, name: &str, is_dir: bool) -> String {
    let full = full_path(current, name);
    if is_dir {
        format!("/sites/{}/files?path={}", site_id, encode_path(&full))
    } else {
        href_read(site_id, current, name)
    }
}

fn href_read(site_id: Uuid, current: &str, name: &str) -> String {
    format!(
        "/sites/{}/files/read?path={}",
        site_id,
        encode_path(&full_path(current, name))
    )
}

fn href_mkdir(site_id: Uuid) -> String {
    format!("/sites/{site_id}/files/mkdir")
}

fn href_rename(site_id: Uuid) -> String {
    format!("/sites/{site_id}/files/rename")
}

fn href_chmod(site_id: Uuid) -> String {
    format!("/sites/{site_id}/files/chmod")
}

fn href_write(site_id: Uuid) -> String {
    format!("/sites/{site_id}/files/write")
}

fn href_upload(site_id: Uuid) -> String {
    format!("/sites/{site_id}/files/upload")
}

fn full_path(current: &str, name: &str) -> String {
    if current.is_empty() {
        name.to_string()
    } else {
        format!("{current}/{name}")
    }
}

fn collect_rows(entries: &[FileInfo]) -> Vec<FileRow> {
    let mut rows = Vec::with_capacity(entries.len());
    for e in entries {
        rows.push(FileRow {
            name: e.name.clone(),
            is_dir: e.is_dir,
            size: if e.is_dir {
                "—".into()
            } else {
                human_size(e.size)
            },
            mode: e.mode.clone(),
            mtime: format_dt(e.mtime),
            mime: e.mime.clone(),
        });
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{:.1} {}", v, UNITS[u])
}

fn format_dt(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M UTC").to_string()
}

/// Best-effort text detection: an empty file is text; anything with
/// a NUL byte is binary; everything else is text.
fn is_probably_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0u8)
}

use axum::Form;

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use openpanel_domain::files::file_info::FileInfo;

    use super::*;

    fn info(name: &str, is_dir: bool, size: u64) -> FileRow {
        FileRow {
            name: name.into(),
            is_dir,
            size: if is_dir {
                "—".into()
            } else {
                human_size(size)
            },
            mode: if is_dir { "755" } else { "644" }.into(),
            mtime: format_dt(Utc::now()),
            mime: None,
        }
    }

    #[test]
    fn listing_renders_one_row_per_entry() {
        let rows = vec![info("a.txt", false, 100), info("subdir", true, 0)];
        let out = listing_fragment(Uuid::new_v4(), "", &rows, "tok").into_string();
        assert!(out.contains("a.txt"), "a.txt: {out}");
        assert!(out.contains("subdir"), "subdir: {out}");
        assert!(out.contains("dir"), "type dir: {out}");
        assert!(out.contains("file"), "type file: {out}");
    }

    #[test]
    fn listing_renders_size_and_mode() {
        let rows = vec![info("big.bin", false, 2048)];
        let out = listing_fragment(Uuid::new_v4(), "", &rows, "tok").into_string();
        assert!(out.contains("2.0 KB"), "size: {out}");
        assert!(out.contains("644"), "mode: {out}");
    }

    #[test]
    fn listing_renders_url_encoded_hrefs() {
        let rows = vec![info("with space.txt", false, 0)];
        let out = listing_fragment(Uuid::new_v4(), "", &rows, "tok").into_string();
        assert!(out.contains("with%20space.txt"), "encoded: {out}");
    }

    #[test]
    fn listing_renders_delete_via_layer_confirm() {
        let rows = vec![info("a.txt", false, 0)];
        let out = listing_fragment(Uuid::new_v4(), "", &rows, "tok").into_string();
        assert!(
            out.contains("action=delete-entry&amp;site_id="),
            "delete routes through layer confirm: {out}"
        );
        assert!(out.contains("path=a.txt"), "path forwarded: {out}");
        assert!(!out.contains("hx-confirm"), "no native confirm: {out}");
        assert!(!out.contains("hx-delete="), "no raw delete: {out}");
    }

    #[test]
    fn listing_empty_state_renders() {
        let out = listing_fragment(Uuid::new_v4(), "", &[], "tok").into_string();
        assert!(out.contains("Empty directory"), "empty: {out}");
    }

    #[test]
    fn breadcrumb_renders_segments() {
        let site = Uuid::new_v4();
        let out = breadcrumb(site, "wp-content/themes", &Some("wp-content".into())).into_string();
        assert!(out.contains("root"), "root: {out}");
        assert!(out.contains("wp-content"), "wp-content: {out}");
        assert!(out.contains("themes"), "themes: {out}");
    }

    #[test]
    fn parent_of_returns_parent_path() {
        assert_eq!(parent_of("a/b/c"), Some("a/b".into()));
        assert_eq!(parent_of("a"), Some(String::new()));
        assert_eq!(parent_of(""), None);
    }

    #[test]
    fn encode_segment_escapes_specials() {
        assert_eq!(encode_segment("plain"), "plain");
        assert_eq!(encode_segment("a b"), "a%20b");
        assert_eq!(encode_segment("a/b"), "a%2Fb");
    }

    #[test]
    fn encode_path_preserves_separators() {
        assert_eq!(encode_path("a/b c/d"), "a/b%20c/d");
    }

    #[test]
    fn human_size_picks_unit() {
        assert_eq!(human_size(0), "0.0 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn is_probably_text_recognises_nul_as_binary() {
        assert!(is_probably_text(b"hello"));
        assert!(is_probably_text(b""));
        assert!(!is_probably_text(b"hel\0lo"));
    }

    #[test]
    fn editor_panel_renders_textarea_with_contents() {
        let out = editor_panel(
            Uuid::new_v4(),
            "hello.txt",
            "hello world",
            Utc::now(),
            "tok",
        )
        .into_string();
        assert!(out.contains("name=\"contents\""), "textarea: {out}");
        assert!(out.contains("hello world"), "contents: {out}");
        assert!(out.contains("name=\"path\""), "path: {out}");
        assert!(out.contains("value=\"hello.txt\""), "path value: {out}");
    }

    #[test]
    fn error_region_renders_alert() {
        let out = error_region("boom").into_string();
        assert!(out.contains("role=\"alert\""), "alert: {out}");
        assert!(out.contains("boom"), "message: {out}");
    }

    #[test]
    fn collect_rows_sorts_by_name() {
        let entries = vec![
            FileInfo::new("b", false, 0, "644", Utc::now(), None),
            FileInfo::new("a", false, 0, "644", Utc::now(), None),
            FileInfo::new("c", true, 0, "755", Utc::now(), None),
        ];
        let rows = collect_rows(&entries);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }
}
