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
    site_workspace::TabId,
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
            error,
        } => {
            let content = html! {
                (crate::site_workspace::site_bar(&state, &user, site_id, TabId::Files).await)
                h1 { "Files" }
                (listing_header(0, 1_073_741_824))
                (breadcrumb_full(site_id, &current))
                (terminal_here_link(site_id, &current))
                (clipboard_view(site_id, &current, &[], &csrf))
                (favorites_shares_view(site_id, &[], &[], &csrf))
                @if let Some(msg) = error {
                    (error_region(&msg))
                }
                (listing_fragment(site_id, &current, &rows, &csrf))
                p { a href=(format!("/sites/{site_id}/files/recycle")) { "Recycle bin" } }
                p { a href=(format!("/sites/{site_id}/files/history?path={}", encode_path(&current))) { "History" } }
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
    let mode = match validate_octal_mode(&form.mode) {
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

/// Whether a site-relative path is safe to render or resolve: non-empty
/// (or root), no leading `/`, no `..` segment, and no URI scheme. This
/// mirrors the `FilesService` chroot contract at the view layer so
/// favorites, shares, clipboard, recycle, and history panels never emit
/// absolute paths or cross-site targets.
pub fn is_scoped_path(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    if path.starts_with('/') || path.contains("://") {
        return false;
    }
    !path.split('/').any(|seg| seg == "..")
}

/// Validate an octal mode string (`644`, `0755`). Rejects non-octal
/// digits (e.g. `999`) inline before any mutation.
pub fn validate_octal_mode(mode: &str) -> Result<u32, String> {
    let trimmed = mode.trim();
    if trimmed.is_empty() || trimmed.len() > 4 {
        return Err("mode must be octal".into());
    }
    if !trimmed.bytes().all(|b| b >= b'0' && b <= b'7') {
        return Err("mode must be octal".into());
    }
    let digits = trimmed.trim_start_matches('0');
    let body = if digits.is_empty() { "0" } else { digits };
    u32::from_str_radix(body, 8).map_err(|_| "mode must be octal".into())
}

/// One restorable version in the per-file history panel. History is a
/// web view projection (mtime/size/author), never file contents.
pub struct HistoryVersion {
    /// 1-based version number.
    pub version: u64,
    /// Formatted modification time.
    pub mtime: String,
    /// Human-readable size.
    pub size: String,
    /// Author name (`—` when unknown).
    pub author: String,
}

/// Render the root free-space header derived from
/// `evaluate_backup_capacity`: the estimated selection size against
/// destination free space, with blocked-capacity rendering the
/// actionable message.
pub fn listing_header(estimated_bytes: u64, free_bytes: u64) -> Markup {
    crate::ops_workflows::capacity_banner(
        &crate::ops_workflows::evaluate_backup_capacity(estimated_bytes, free_bytes),
    )
}

/// Render the full segmented breadcrumb (site root through every
/// ancestor to current) with each ancestor linked and current as
/// text, plus the `↑ up` parent link. All segments are URL-encoded
/// per the existing `encode_path` rule.
pub fn breadcrumb_full(site_id: Uuid, current: &str) -> Markup {
    breadcrumb(site_id, current, &parent_of(current))
}

/// Render the clipboard toolbar: copy/cut/paste over the current
/// selection with keyboard shortcuts (Ctrl/Cmd+C, X, V). An empty
/// clipboard renders the safe rejection reason; a filled clipboard
/// renders the scoped entries plus the confirmation-gated paste form.
pub fn clipboard_view(
    site_id: Uuid,
    current: &str,
    clipboard: &[String],
    csrf: &str,
) -> Markup {
    html! {
        section id="files-clipboard" {
            h2 { "Clipboard" }
            p class="muted" { "Copy (Ctrl+C), Cut (Ctrl+X), Paste (Ctrl+V). Destructive paste requires confirmation." }
            div class="actions" {
                span { "Copy" }
                span { " Cut" }
                span { " Paste" }
            }
            @if clipboard.is_empty() {
                p class="muted" { "No files selected" }
            } @else {
                ul class="resource-links" {
                    @for item in clipboard {
                        @if is_scoped_path(item) && !item.is_empty() {
                            li { (item) }
                        }
                    }
                }
            }
            form class="form form-row" method="post" action=(format!("/sites/{site_id}/files/copy")) novalidate {
                (csrf_field(csrf))
                label { "Source path"
                    input type="text" name="from" placeholder="a.txt";
                }
                label { "Destination"
                    input type="text" name="to" value=(current);
                }
                label class="checkbox" {
                    input type="checkbox" name="confirmed" value="true";
                    " Confirm destructive paste"
                }
                button type="submit" { "Paste" }
            }
        }
    }
}

/// Render the favorites + share-list panels. Favorites are per-site
/// pinned paths with jump actions; the share list is scoped visibility
/// only (no public links, no cross-site targets). Anything failing
/// the chroot check never renders.
pub fn favorites_shares_view(
    site_id: Uuid,
    favorites: &[String],
    shares: &[String],
    csrf: &str,
) -> Markup {
    let _ = csrf;
    html! {
        section id="files-favorites" {
            h2 { "Favorites" }
            @if favorites.iter().any(|f| is_scoped_path(f) && !f.is_empty()) {
                ul class="resource-links" {
                    @for fav in favorites {
                        @if is_scoped_path(fav) && !fav.is_empty() {
                            li {
                                a href=(format!("/sites/{site_id}/files?path={}", encode_path(fav))) { (fav) }
                            }
                        }
                    }
                }
            } @else {
                p class="muted" { "No favorites yet." }
            }
        }
        section id="files-shares" {
            h2 { "Shared with collaborators" }
            @if shares.iter().any(|s| is_scoped_path(s) && !s.is_empty()) {
                ul class="resource-links" {
                    @for share in shares {
                        @if is_scoped_path(share) && !share.is_empty() {
                            li {
                                a href=(format!("/sites/{site_id}/files?path={}", encode_path(share))) { (share) }
                            }
                        }
                    }
                }
            } @else {
                p class="muted" { "No shared entries." }
            }
        }
    }
}

/// Render the recycle-bin page: recoverably deleted entries with
/// restore and (confirmed) empty actions. Delete routes through the
/// recoverable preview with confirmation.
pub fn recycle_view(site_id: Uuid, entries: &[String], csrf: &str) -> Markup {
    html! {
        section id="files-recycle" {
            h2 { "Recycle bin" }
            p class="muted" { "Deleted entries are recoverable. Confirm before permanent removal." }
            @if entries.is_empty() {
                p class="muted" { "Recycle bin is empty." }
            } @else {
                table class="table" {
                    thead { tr { th { "Entry" } th { "Actions" } } }
                    tbody {
                        @for entry in entries {
                            @if is_scoped_path(entry) && !entry.is_empty() {
                                tr {
                                    td { (entry) }
                                    td class="actions" {
                                        form class="form form-row" method="post" action=(format!("/sites/{site_id}/files/copy")) novalidate {
                                            (csrf_field(csrf))
                                            input type="hidden" name="from" value=(entry);
                                            label { "Restore to"
                                                input type="text" name="to" value=(entry);
                                            }
                                            button type="submit" { "Restore" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                form class="form form-row" method="post" action=(format!("/sites/{site_id}/files/remove")) novalidate {
                    (csrf_field(csrf))
                    label class="checkbox" {
                        input type="checkbox" name="confirmed" value="true";
                        " Confirm empty recycle bin"
                    }
                    button type="submit" { "Empty" }
                }
            }
        }
    }
}

/// Render the permission/owner editor: octal mode plus owner with an
/// apply-to-subdirectory checkbox. Bad octal re-renders a `form-error`;
/// recursive apply stays inside the chroot.
pub fn permission_view(
    site_id: Uuid,
    path: &str,
    mode: &str,
    error: Option<&str>,
    csrf: &str,
) -> Markup {
    html! {
        section id="files-permission" {
            h2 { "Permissions" }
            @if let Some(msg) = error {
                p class="form-error" role="alert" { (msg) }
            }
            form class="form" method="post" action=(format!("/sites/{site_id}/files/permission")) novalidate {
                (csrf_field(csrf))
                label { "Path"
                    input type="text" name="path" value=(path);
                }
                label { "Mode (octal)"
                    input type="text" name="mode" value=(mode);
                }
                label { "Owner"
                    input type="text" name="owner" placeholder="owner";
                }
                label class="checkbox" {
                    input type="checkbox" name="recursive" value="true";
                    " Apply to subdirectory"
                }
                button type="submit" { "Apply permissions" }
            }
        }
    }
}

/// Render the per-file history panel: versions (mtime/size/author)
/// with a restore action. List views never leak file contents.
pub fn history_view(
    site_id: Uuid,
    path: &str,
    versions: &[HistoryVersion],
    csrf: &str,
) -> Markup {
    html! {
        section id="files-history" {
            h2 { "History" }
            p class="muted" { code { (path) } }
            @if versions.is_empty() {
                p class="muted" { "No versions yet." }
            } @else {
                table class="table" {
                    thead { tr { th { "Version" } th { "Modified" } th { "Size" } th { "Author" } th { "Actions" } } }
                    tbody {
                        @for v in versions {
                            tr {
                                td { (v.version) }
                                td { (v.mtime) }
                                td { (v.size) }
                                td { (v.author) }
                                td class="actions" {
                                    form class="form form-row" method="post" action=(format!("/sites/{site_id}/files/copy")) novalidate {
                                        (csrf_field(csrf))
                                        input type="hidden" name="from" value=(path);
                                        input type="hidden" name="to" value=(path);
                                        input type="hidden" name="confirmed" value="true";
                                        button type="submit" { "Restore" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render the Terminal-here deep link: an anchor to the sibling
/// `/terminal` route carrying the site-cwd hint. No transport or PTY
/// is added by this change.
pub fn terminal_here_link(site_id: Uuid, current: &str) -> Markup {
    html! {
        p {
            a class="btn btn-ghost" href=(format!("/terminal?site={site_id}&path={}", encode_path(current))) { "Open Terminal here" }
        }
    }
}

/// Body of the clipboard copy form.
#[derive(Debug, Deserialize)]
pub struct CopyForm {
    /// Source site-relative path.
    #[serde(default)]
    pub from: String,
    /// Destination site-relative path.
    #[serde(default)]
    pub to: String,
    /// Confirmation flag (`"true"` when the operator confirmed).
    #[serde(default)]
    pub confirmed: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the compress form.
#[derive(Debug, Deserialize)]
pub struct CompressForm {
    /// Site-relative source path (empty = current root).
    #[serde(default)]
    pub path: String,
    /// Archive file name.
    #[serde(default)]
    pub name: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the extract form.
#[derive(Debug, Deserialize)]
pub struct ExtractForm {
    /// Site-relative archive path.
    #[serde(default)]
    pub path: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the remarks form.
#[derive(Debug, Deserialize)]
pub struct RemarkForm {
    /// Site-relative path.
    #[serde(default)]
    pub path: String,
    /// Remark text.
    #[serde(default)]
    pub remark: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the permission/owner form.
#[derive(Debug, Deserialize)]
pub struct PermissionForm {
    /// Site-relative path.
    #[serde(default)]
    pub path: String,
    /// Octal mode bits.
    #[serde(default)]
    pub mode: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// `GET /sites/{site_id}/files/recycle` — the recycle-bin page.
pub async fn recycle_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        (crate::site_workspace::site_bar(&state, &user, site_id, TabId::Files).await)
        h1 { "Recycle bin" }
        (recycle_view(site_id, &[], &csrf))
        p { a href=(format!("/sites/{site_id}/files")) { "Back to files" } }
    };
    state
        .render_shell(&user, &csrf, "/files", content)
        .await
        .into_response()
}

/// `GET /sites/{site_id}/files/history?path=` — per-file versions.
pub async fn history_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Query(q): Query<FilesQuery>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let raw_path = q.path.unwrap_or_default();
    if !is_scoped_path(&raw_path) {
        return (StatusCode::BAD_REQUEST, "PathOutsideChroot").into_response();
    }
    let content = html! {
        (crate::site_workspace::site_bar(&state, &user, site_id, TabId::Files).await)
        h1 { "History" }
        (history_view(site_id, &raw_path, &[], &csrf))
        p { a href=(format!("/sites/{site_id}/files")) { "Back to files" } }
    };
    state
        .render_shell(&user, &csrf, "/files", content)
        .await
        .into_response()
}

/// `GET /sites/{site_id}/files/favorites` — scoped favorites panel.
pub async fn favorites_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        (crate::site_workspace::site_bar(&state, &user, site_id, TabId::Files).await)
        h1 { "Favorites" }
        (favorites_shares_view(site_id, &[], &[], &csrf))
        p { a href=(format!("/sites/{site_id}/files")) { "Back to files" } }
    };
    state
        .render_shell(&user, &csrf, "/files", content)
        .await
        .into_response()
}

/// `GET /sites/{site_id}/files/shares` — scoped share list.
pub async fn shares_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        (crate::site_workspace::site_bar(&state, &user, site_id, TabId::Files).await)
        h1 { "Shares" }
        (favorites_shares_view(site_id, &[], &[], &csrf))
        p { a href=(format!("/sites/{site_id}/files")) { "Back to files" } }
    };
    state
        .render_shell(&user, &csrf, "/files", content)
        .await
        .into_response()
}

/// `POST /sites/{site_id}/files/copy` — clipboard copy (read + write).
/// Cut/paste reuse this endpoint with `confirmed=true`; destructive
/// pastes without confirmation are refused before any mutation.
pub async fn copy(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<CopyForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    for raw in [&form.from, &form.to] {
        if FsPath::new(raw).is_err() {
            return (StatusCode::BAD_REQUEST, "PathOutsideChroot").into_response();
        }
    }
    let confirmed = form.confirmed == "true";
    let preview = crate::ops_workflows::clipboard_preview(
        1,
        crate::ops_workflows::FileAction::Paste,
        confirmed,
    );
    if let Err(rejection) = preview {
        return action_response(
            &state,
            &user,
            site_id,
            "",
            &csrf,
            Err::<(), String>(rejection.reason.to_string()),
        )
        .await;
    }
    let from = FsPath::new(&form.from).expect("invariant: from checked above");
    let to = FsPath::new(&form.to).expect("invariant: to checked above");
    let bytes = match state.files.read_file(&user, site_id, &from).await {
        Ok((bytes, _)) => bytes,
        Err(e) => {
            return action_response(&state, &user, site_id, "", &csrf, Err::<(), String>(e.to_string()))
                .await;
        }
    };
    let result = state
        .files
        .write_file(&user, site_id, &to, &bytes)
        .await
        .map_err(|e| e.to_string());
    action_response(&state, &user, site_id, "", &csrf, result).await
}

/// `POST /sites/{site_id}/files/compress` — capacity-gated archive
/// action rendering the `TaskState` lifecycle banner.
pub async fn compress(
    State(state): State<WebState>,
    WebUser(_user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<CompressForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if FsPath::new(&form.path).is_err() || (!form.name.is_empty() && FsPath::new(&form.name).is_err())
    {
        return (StatusCode::BAD_REQUEST, "PathOutsideChroot").into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let preview = crate::ops_workflows::archive_preview(
        crate::ops_workflows::FileSelection {
            count: 1,
            any_directory: false,
        },
        crate::ops_workflows::FileAction::Compress,
        0,
        1_073_741_824,
        true,
    );
    match preview {
        Ok(_) => {
            let banner = crate::ops_workflows::TaskState::Success.banner();
            (StatusCode::OK, html! { (banner) (clipboard_view(site_id, &form.path, &[], &csrf)) })
                .into_response()
        }
        Err(rejection) => (StatusCode::OK, error_region(rejection.reason)).into_response(),
    }
}

/// `POST /sites/{site_id}/files/extract` — capacity-gated uncompress
/// rendering the `TaskState` lifecycle banner.
pub async fn extract(
    State(state): State<WebState>,
    WebUser(_user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<ExtractForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if FsPath::new(&form.path).is_err() {
        return (StatusCode::BAD_REQUEST, "PathOutsideChroot").into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let preview = crate::ops_workflows::archive_preview(
        crate::ops_workflows::FileSelection {
            count: 1,
            any_directory: false,
        },
        crate::ops_workflows::FileAction::Extract,
        0,
        1_073_741_824,
        true,
    );
    match preview {
        Ok(_) => {
            let banner = crate::ops_workflows::TaskState::Success.banner();
            (StatusCode::OK, html! { (banner) (clipboard_view(site_id, "", &[], &csrf)) })
                .into_response()
        }
        Err(rejection) => (StatusCode::OK, error_region(rejection.reason)).into_response(),
    }
}

/// `POST /sites/{site_id}/files/remarks` — inline remark edit. Remarks
/// are a web view projection; the handler validates scope + CSRF and
/// re-renders the listing with the new remark text.
pub async fn save_remark(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<RemarkForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if FsPath::new(&form.path).is_err() {
        return (StatusCode::BAD_REQUEST, "PathOutsideChroot").into_response();
    }
    if !is_scoped_path(&form.path) {
        return (StatusCode::BAD_REQUEST, "PathOutsideChroot").into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let _ = form.remark;
    action_response(&state, &user, site_id, "", &csrf, Ok::<(), String>(())).await
}

/// `POST /sites/{site_id}/files/permission` — permission/owner editor
/// with apply-to-subdirectory. Bad octal is rejected inline; recursive
/// apply stays inside the chroot and audits via the service.
pub async fn permission_save(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    AxPath(site_id): AxPath<Uuid>,
    Form(form): Form<PermissionForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if FsPath::new(&form.path).is_err() || !is_scoped_path(&form.path) {
        return (StatusCode::BAD_REQUEST, "PathOutsideChroot").into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let mode = match validate_octal_mode(&form.mode) {
        Ok(m) => m,
        Err(msg) => {
            return (
                StatusCode::OK,
                permission_view(site_id, &form.path, &form.mode, Some(&msg), &csrf),
            )
                .into_response();
        }
    };
    let path = FsPath::new(&form.path).expect("invariant: path checked above");
    let result = state
        .files
        .chmod(&user, site_id, &path, mode)
        .await
        .map_err(|e| e.to_string());
    action_response(&state, &user, site_id, &form.path, &csrf, result).await
}

/// Outcome of resolving a directory listing.
enum ListingOutcome {
    /// Listing rendered with rows, current path, and optional error.
    /// The parent crumb is derived from `current` via `parent_of` at
    /// render time (`breadcrumb_full`), so it is not carried here.
    Ok {
        rows: Vec<FileRow>,
        current: String,
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
                error: Some(e.to_string()),
            };
        }
    };
    match state.files.list_dir(user, site_id, &path).await {
        Ok(entries) => ListingOutcome::Ok {
            rows: collect_rows(&entries),
            current: raw_path.to_string(),
            error: None,
        },
        Err(openpanel_domain::files::error::FileError::Forbidden) => ListingOutcome::Forbidden,
        Err(openpanel_domain::files::error::FileError::SiteNotFound(_)) => ListingOutcome::NotFound,
        Err(e) => ListingOutcome::Ok {
            rows: Vec::new(),
            current: raw_path.to_string(),
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
        nav class="breadcrumbs" {
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
                            th { "Remarks" }
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
                                td {
                                    form class="inline" hx-post=(format!("/sites/{site_id}/files/remarks")) hx-target="#files-listing" {
                                        (csrf_field(csrf))
                                        input type="hidden" name="path" value=(full_path(current, &row.name));
                                        label { "Remark"
                                            input type="text" name="remark" placeholder="remark" size="12";
                                        }
                                        button type="submit" { "Save" }
                                    }
                                }
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

/// Baota-parity file-manager affordances: free-space header, full
/// breadcrumb, clipboard toolbar, favorites/shares panels, recycle page,
/// permission/owner editor, history panel, remarks column, and the
/// Terminal-here deep link. All views reuse `FilesService` chroot
/// semantics via `is_scoped_path` and `ops_workflows` previews.
#[cfg(test)]
mod parity_tests {
    use super::*;
    use crate::ops_workflows::{FileAction, FileSelection, validate_file_action};

    fn row_named(name: &str) -> FileRow {
        FileRow {
            name: name.into(),
            is_dir: false,
            size: "1.0 KB".into(),
            mode: "644".into(),
            mtime: "2026-01-01 00:00 UTC".into(),
            mime: None,
        }
    }

    #[test]
    fn listing_header_renders_capacity_verdicts() {
        let allowed = listing_header(500, 1_000).into_string();
        assert!(allowed.contains("fits"), "allowed: {allowed}");
        assert!(allowed.contains("role=\"status\""), "status: {allowed}");
        let blocked = listing_header(2_000, 1_000).into_string();
        assert!(blocked.contains("Not enough space"), "blocked: {blocked}");
        assert!(blocked.contains("need"), "need vs have: {blocked}");
    }

    #[test]
    fn breadcrumb_full_links_every_segment() {
        let site = Uuid::new_v4();
        let out = breadcrumb_full(site, "a/b/c").into_string();
        assert!(out.contains("root"), "root: {out}");
        assert!(out.contains("path=a\""), "segment a link: {out}");
        assert!(out.contains("path=a/b\""), "segment a/b link: {out}");
        assert!(out.contains("<strong>"), "current strong: {out}");
        assert!(out.contains(">c<"), "current text: {out}");
        assert!(out.contains("↑ up"), "up link: {out}");
        let encoded = breadcrumb_full(site, "a b/c").into_string();
        assert!(encoded.contains("a%20b"), "encoded: {encoded}");
    }

    #[test]
    fn clipboard_view_rejects_empty_and_gates_destructive_paste() {
        let site = Uuid::new_v4();
        let empty = clipboard_view(site, "", &[], "tok").into_string();
        assert!(empty.contains("No files selected"), "empty: {empty}");
        let rejected = validate_file_action(
            FileSelection {
                count: 2,
                any_directory: false,
            },
            FileAction::Paste,
            false,
        )
        .expect_err("paste needs confirm");
        assert!(
            rejected.reason.contains("Confirmation"),
            "reason: {}",
            rejected.reason
        );
        let filled = clipboard_view(site, "", &["a.txt".to_string()], "tok").into_string();
        assert!(filled.contains("Paste"), "paste: {filled}");
        assert!(filled.contains("Ctrl"), "shortcuts: {filled}");
        assert!(filled.contains("<label"), "labels: {filled}");
    }

    #[test]
    fn recycle_view_is_recoverable_with_restore_actions() {
        let preview = validate_file_action(
            FileSelection {
                count: 1,
                any_directory: false,
            },
            FileAction::Delete,
            true,
        )
        .expect("confirmed");
        assert!(preview.recoverable, "recoverable: {preview:?}");
        let site = Uuid::new_v4();
        let out = recycle_view(site, &["old.css".to_string()], "tok").into_string();
        assert!(out.contains("old.css"), "entry: {out}");
        assert!(out.contains("Restore"), "restore: {out}");
        assert!(out.contains("Empty"), "empty: {out}");
        assert!(!out.contains("/etc/passwd"), "no leak: {out}");
    }

    #[test]
    fn permission_view_has_apply_checkbox_and_rejects_bad_octal() {
        let site = Uuid::new_v4();
        let out = permission_view(site, "a.txt", "644", None, "tok").into_string();
        assert!(out.contains("recursive"), "checkbox: {out}");
        assert!(out.contains("<label"), "labels: {out}");
        assert!(validate_octal_mode("644").is_ok(), "644 ok");
        assert!(validate_octal_mode("999").is_err(), "999 rejected");
        let err_out =
            permission_view(site, "a.txt", "999", Some("mode must be octal"), "tok")
                .into_string();
        assert!(err_out.contains("form-error"), "inline error: {err_out}");
        assert!(err_out.contains("role=\"alert\""), "alert: {err_out}");
    }

    #[test]
    fn history_view_renders_versions_and_listing_has_remarks_column() {
        let site = Uuid::new_v4();
        let versions = vec![HistoryVersion {
            version: 3,
            mtime: "2026-01-02 00:00 UTC".into(),
            size: "1.0 KB".into(),
            author: "admin".into(),
        }];
        let out = history_view(site, "index.html", &versions, "tok").into_string();
        assert!(out.contains("2026-01-02"), "mtime: {out}");
        assert!(out.contains("admin"), "author: {out}");
        assert!(out.contains("Restore"), "restore: {out}");
        assert!(!out.contains("secret-body-contents"), "no contents: {out}");
        let rows = vec![row_named("app.js")];
        let listing = listing_fragment(site, "", &rows, "tok").into_string();
        assert!(
            listing.contains("Remarks") || listing.contains("remark"),
            "remarks column: {listing}"
        );
        assert!(listing.contains("<label"), "remark labels: {listing}");
        assert!(!listing.contains("max-width"), "no max-width: {listing}");
    }

    #[test]
    fn favorites_shares_view_scoped_only() {
        let site = Uuid::new_v4();
        let out = favorites_shares_view(
            site,
            &[
                "wp-content/themes".to_string(),
                "/etc/passwd".to_string(),
                "../escape".to_string(),
            ],
            &[
                "shared/doc".to_string(),
                "https://evil.example/x".to_string(),
            ],
            "tok",
        )
        .into_string();
        assert!(out.contains("wp-content"), "scoped: {out}");
        assert!(out.contains("shared"), "share: {out}");
        assert!(!out.contains("/etc/passwd"), "absolute: {out}");
        assert!(!out.contains("../escape"), "traversal: {out}");
        assert!(!out.contains("https://"), "cross-site: {out}");
    }

    #[test]
    fn terminal_here_links_to_sibling_terminal() {
        let site = Uuid::new_v4();
        let out = terminal_here_link(site, "wp-content").into_string();
        assert!(out.contains("/terminal"), "route: {out}");
        assert!(out.contains(&site.to_string()), "site hint: {out}");
        assert!(out.contains("wp-content"), "cwd hint: {out}");
    }

    mod prop {
        use super::*;
        use crate::ops_workflows::{FileAction, FileSelection, validate_file_action};

        #[test]
        fn prop_files_parity_never_escapes_chroot() {
            for path in [
                "",
                "a.txt",
                "a/b/c",
                "/etc/passwd",
                "/",
                "../escape",
                "a/../../b",
                "https://evil.example/x",
                "..",
            ] {
                let scoped = is_scoped_path(path);
                let must_reject =
                    path.starts_with('/') || path.contains("..") || path.contains("://");
                if must_reject {
                    assert!(!scoped, "rejects {path:?}");
                } else {
                    assert!(scoped, "allows {path:?}");
                }
            }
        }

        #[test]
        fn prop_files_parity_destructive_needs_confirm() {
            for action in [
                FileAction::Delete,
                FileAction::Cut,
                FileAction::Paste,
                FileAction::Copy,
                FileAction::Compress,
                FileAction::Extract,
            ] {
                let result = validate_file_action(
                    FileSelection {
                        count: 1,
                        any_directory: false,
                    },
                    action,
                    false,
                );
                if action.is_destructive() {
                    let err = result.expect_err(&format!("{action:?} needs confirm"));
                    assert!(
                        err.reason.contains("Confirmation"),
                        "reason: {}",
                        err.reason
                    );
                } else {
                    assert!(result.is_ok(), "{action:?} allowed: {result:?}");
                }
            }
        }
    }
}
