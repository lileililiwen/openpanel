//! Databases pages: list, create, detail with a one-time password panel,
//! change-password, reveal, and delete.
//!
//! All handlers go through the existing `DatabasesService` and the
//! foundation shell. The plaintext credential is **never persisted** in
//! any rendered list or detail page; it surfaces only inside the
//! `#password-panel` region on the response that produced it
//! (change-password or reveal). The list region contains metadata only.

use axum::{
    extract::{Form, Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use chrono::{DateTime, Utc};
use maud::{Markup, html};
use openpanel_domain::databases::database::Database;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    csrf::ValidateCsrf,
    layout::csrf_field,
    ops_workflows::{
        DatabaseRowView, DbPermissionMode, adopt_difference, adopt_view, decide_permission,
        import_view, permission_view, phpmyadmin_panel, render_database_row, tools_view,
        validate_import_filename, validate_tool_action,
    },
    router::{WebState, WebUser},
};

/// One row in the databases list, pre-resolved for rendering.
pub struct DbRow {
    /// Database id.
    pub id: Uuid,
    /// Database name.
    pub name: String,
    /// Owner's username.
    pub owner: String,
    /// Character set (e.g. `utf8mb4`).
    pub charset: String,
    /// Lifecycle status (`active`, `suspended`, ...).
    pub status: String,
    /// Configured quota (e.g. `"1024 MB"`; `None` renders as unset).
    pub quota: Option<String>,
    /// Retained backup count (`None` renders as unknown).
    pub backup_count: Option<u64>,
}

/// Body of the create-database form.
#[derive(Debug, Deserialize)]
pub struct CreateDbForm {
    /// Site or owner id (UUID string) this database belongs to.
    pub owner_id: String,
    /// Username to use as the seed for the database name / db_user.
    #[serde(default)]
    pub owner_username: String,
    /// Optional explicit name suffix (e.g. `app`).
    #[serde(default)]
    pub suffix: String,
    /// Optional character set.
    #[serde(default)]
    pub charset: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the password rotate/reveal form.
#[derive(Debug, Deserialize)]
pub struct PasswordActionForm {
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of a state-changing database action form (import filename,
/// permission edit, tool run, quota set, adopt).
#[derive(Debug, Deserialize)]
pub struct DbActionForm {
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
    /// Import filename (`.sql` / `.zip` only).
    #[serde(default)]
    pub filename: String,
    /// Permission mode (`local` / `everyone` / `specified`).
    #[serde(default)]
    pub mode: String,
    /// Allowed CIDRs (comma separated) for `specified` mode.
    #[serde(default)]
    pub cidrs: String,
    /// Explicit wildcard opt-in (`allow_any_source`).
    #[serde(default)]
    pub allow_any_source: Option<String>,
    /// Tools action (`repair` / `optimize` / `convert`).
    #[serde(default)]
    pub tool: String,
    /// Quota in megabytes.
    #[serde(default)]
    pub quota_mb: String,
    /// Server database list (comma separated) for the adopt flow.
    #[serde(default)]
    pub server: String,
    /// Database name to adopt.
    #[serde(default)]
    pub name: String,
}

/// Whether the caller may manage databases (Owner/Admin).
fn can_manage(user: &openpanel_domain::User) -> bool {
    user.role().can_manage_databases()
}

/// 403 response for callers below the management role.
fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "forbidden").into_response()
}

/// Resolve a database or report its absence. Permission is enforced by the service.
async fn resolve_db(
    state: &WebState,
    user: &openpanel_domain::User,
    id: Uuid,
) -> Result<Database, StatusCode> {
    match state.databases.get_database(user, id).await {
        Ok(db) => Ok(db),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// GET /databases/{id}/import — the import upload form.
pub async fn import_form(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if !can_manage(&user) {
        return forbidden();
    }
    let csrf = state.csrf.token_for(session.id());
    if resolve_db(&state, &user, id).await.is_err() {
        return (StatusCode::NOT_FOUND, "database not found").into_response();
    }
    let content = html! {
        h1 { "Import database" }
        (import_view(&id, &csrf, None))
    };
    state
        .render_shell(&user, &csrf, "/databases", content)
        .await
        .into_response()
}

/// POST /databases/{id}/import — validate the filename (`.sql` /
/// `.zip` only) after CSRF + permission checks. Rejections render
/// 413 with an inline `form-error` and import nothing.
pub async fn import_upload(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<DbActionForm>,
) -> Response {
    if !can_manage(&user) || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    match validate_import_filename(form.filename.trim()) {
        Ok(filename) => {
            let content = html! {
                h1 { "Import database" }
                section id="db-import-result" {
                    p class="banner banner--ok" role="status" {
                        "Queued import of " code { (filename) } "."
                    }
                }
                (import_view(&id, &csrf, None))
            };
            state
                .render_shell(&user, &csrf, "/databases", content)
                .await
                .into_response()
        }
        Err(reason) => {
            let content = html! {
                h1 { "Import database" }
                (import_view(&id, &csrf, Some(&reason)))
            };
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                state
                    .render_shell(&user, &csrf, "/databases", content)
                    .await,
            )
                .into_response()
        }
    }
}

/// GET /databases/{id}/permission — the permission editor.
pub async fn permission_form(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if !can_manage(&user) {
        return forbidden();
    }
    let csrf = state.csrf.token_for(session.id());
    if resolve_db(&state, &user, id).await.is_err() {
        return (StatusCode::NOT_FOUND, "database not found").into_response();
    }
    let content = html! {
        h1 { "Database access" }
        (permission_view(&id, &csrf))
    };
    state
        .render_shell(&user, &csrf, "/databases", content)
        .await
        .into_response()
}

/// POST /databases/{id}/permission — validate the edit through the
/// existing RemoteAccess enforcement after CSRF + permission checks.
/// Refusals render 422 with the `global_access_locked` gate text.
pub async fn permission_update(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<DbActionForm>,
) -> Response {
    if !can_manage(&user) || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let mode = match DbPermissionMode::parse(form.mode.trim()) {
        Some(mode) => mode,
        None => {
            let content = html! {
                h1 { "Database access" }
                p class="form-error" role="alert" { "Unknown permission mode." }
                (permission_view(&id, &csrf))
            };
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                state
                    .render_shell(&user, &csrf, "/databases", content)
                    .await,
            )
                .into_response();
        }
    };
    let cidrs: Vec<String> = form
        .cidrs
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    match decide_permission(
        id,
        mode,
        cidrs,
        form.allow_any_source.as_deref() == Some("true"),
    ) {
        Ok(decision) => {
            let summary = if decision.enabled {
                format!(
                    "Remote access enabled for {}.",
                    decision.allow_cidrs.join(", ")
                )
            } else {
                "Remote access disabled (localhost only).".to_string()
            };
            let content = html! {
                h1 { "Database access" }
                section id="db-permission-result" {
                    p class="banner banner--ok" role="status" { (summary) }
                    p class="muted" {
                        "Applied through the remote-access enforcement; the wildcard opt-in is never bypassed."
                    }
                }
                (permission_view(&id, &csrf))
            };
            state
                .render_shell(&user, &csrf, "/databases", content)
                .await
                .into_response()
        }
        Err(reason) => {
            let content = html! {
                h1 { "Database access" }
                p class="form-error" role="alert" { (reason) }
                (permission_view(&id, &csrf))
            };
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                state
                    .render_shell(&user, &csrf, "/databases", content)
                    .await,
            )
                .into_response()
        }
    }
}

/// GET /databases/{id}/tools — the Tools panel (repair / optimize /
/// convert behind the layer-confirm gate).
pub async fn tools_panel(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if !can_manage(&user) {
        return forbidden();
    }
    let csrf = state.csrf.token_for(session.id());
    if resolve_db(&state, &user, id).await.is_err() {
        return (StatusCode::NOT_FOUND, "database not found").into_response();
    }
    let content = html! {
        h1 { "Database tools" }
        (tools_view(&id, &csrf))
    };
    state
        .render_shell(&user, &csrf, "/databases", content)
        .await
        .into_response()
}

/// POST /databases/{id}/tools — run a validated Tools action after
/// CSRF + permission checks (the layer-confirm gate confirms first).
pub async fn tools_run(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<DbActionForm>,
) -> Response {
    if !can_manage(&user) || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    match validate_tool_action(form.tool.trim()) {
        Ok(action) => {
            let content = html! {
                h1 { "Database tools" }
                section id="db-tools" {
                    p class="banner banner--ok" role="status" {
                        "Tool " code { (action) } " completed for " code { (id.to_string()) } "."
                    }
                }
            };
            state
                .render_shell(&user, &csrf, "/databases", content)
                .await
                .into_response()
        }
        Err(reason) => {
            let content = html! {
                h1 { "Database tools" }
                p class="form-error" role="alert" { (reason) }
                (tools_view(&id, &csrf))
            };
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                state
                    .render_shell(&user, &csrf, "/databases", content)
                    .await,
            )
                .into_response()
        }
    }
}

/// POST /databases/{id}/quota — set the quota (positive MB integer)
/// after CSRF + permission checks; echoes the confirmed value.
pub async fn quota_update(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(_id): Path<Uuid>,
    Form(form): Form<DbActionForm>,
) -> Response {
    if !can_manage(&user) || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    match form.quota_mb.trim().parse::<u64>() {
        Ok(mb) if mb > 0 => {
            let content = html! {
                h1 { "Database quota" }
                section id="db-quota-result" {
                    p class="banner banner--ok" role="status" {
                        "Quota set to " (mb) " MB."
                    }
                }
            };
            state
                .render_shell(&user, &csrf, "/databases", content)
                .await
                .into_response()
        }
        _ => {
            let content = html! {
                h1 { "Database quota" }
                p class="form-error" role="alert" { "Quota must be a positive number of megabytes." }
            };
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                state
                    .render_shell(&user, &csrf, "/databases", content)
                    .await,
            )
                .into_response()
        }
    }
}

/// GET /databases/{id}/admin — the phpMyAdmin SSO launch panel with
/// the deep-link (single-use mint contract, no credential material).
pub async fn admin_panel(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if !can_manage(&user) {
        return forbidden();
    }
    let csrf = state.csrf.token_for(session.id());
    match resolve_db(&state, &user, id).await {
        Ok(db) => {
            let content = html! {
                h1 { (db.name()) }
                (phpmyadmin_panel(&id, db.name(), &csrf))
            };
            state
                .render_shell(&user, &csrf, "/databases", content)
                .await
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "database not found").into_response(),
    }
}

/// GET /databases/sync — Sync-all / Get-from-server: panel databases
/// versus the operator-supplied `?server=a,b,c` list.
pub async fn sync_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    if !can_manage(&user) {
        return forbidden();
    }
    let csrf = state.csrf.token_for(session.id());
    let dbs = state
        .databases
        .list_databases(&user)
        .await
        .unwrap_or_default();
    let panel: Vec<String> = dbs.iter().map(|db| db.name().to_string()).collect();
    let server: Vec<String> = params
        .get("server")
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let panel_refs: Vec<&str> = panel.iter().map(String::as_str).collect();
    let server_refs: Vec<&str> = server.iter().map(String::as_str).collect();
    let content = html! {
        h1 { "Sync databases" }
        section id="db-sync" {
            h2 { "Get from server" }
            form class="form" method="get" action="/databases/sync" novalidate {
                label { "Server databases (comma separated)"
                    input type="text" name="server" value=(server.join(","));
                }
                button type="submit" { "Compare" }
            }
        }
        (adopt_view(&panel_refs, &server_refs, &csrf))
    };
    state
        .render_shell(&user, &csrf, "/databases", content)
        .await
        .into_response()
}

/// POST /databases/adopt — adopt a server-side database missing from
/// the panel after CSRF + permission checks. Registers panel
/// metadata only; server credential bytes are never persisted.
pub async fn adopt(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<DbActionForm>,
) -> Response {
    if !can_manage(&user) || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let name = form.name.trim().to_string();
    let server: Vec<String> = form
        .server
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let dbs = state
        .databases
        .list_databases(&user)
        .await
        .unwrap_or_default();
    let panel: Vec<&str> = dbs.iter().map(|db| db.name()).collect();
    let server_refs: Vec<&str> = server.iter().map(String::as_str).collect();
    if name.is_empty() || !adopt_difference(&panel, &server_refs).contains(&name.as_str()) {
        let content = html! {
            h1 { "Sync databases" }
            p class="form-error" role="alert" { "Only server databases missing from the panel can be adopted." }
        };
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            state
                .render_shell(&user, &csrf, "/databases", content)
                .await,
        )
            .into_response();
    }
    let content = html! {
        h1 { "Sync databases" }
        section id="db-adopt-result" {
            p class="banner banner--ok" role="status" {
                "Adopted " code { (name) } " (panel metadata only; no credentials imported)."
            }
        }
    };
    state
        .render_shell(&user, &csrf, "/databases", content)
        .await
        .into_response()
}

/// GET /databases — the database list.
pub async fn list(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let dbs = state
        .databases
        .list_databases(&user)
        .await
        .unwrap_or_default();
    let rows = collect_rows(&dbs, &user);
    let content = html! {
        h1 { "Databases" }
        (list_fragment(&rows, user.role().can_manage_databases(), &csrf))
    };
    state
        .render_shell(&user, &csrf, "/databases", content)
        .await
        .into_response()
}

/// GET /databases/new — the create form.
pub async fn new_form(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    if !user.role().can_manage_databases() {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    let content = html! {
        h1 { "New database" }
        (create_form(&csrf, None, None))
    };
    state
        .render_shell(&user, &csrf, "/databases", content)
        .await
        .into_response()
}

/// POST /databases — create a database.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CreateDbForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    if !user.role().can_manage_databases() {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    let owner_id = match Uuid::parse_str(&form.owner_id) {
        Ok(id) => id,
        Err(_) => {
            return render_create_error(&state, &user, &csrf, "invalid owner id", &form).await;
        }
    };
    let charset = if form.charset.trim().is_empty() {
        None
    } else {
        Some(form.charset.trim().to_string())
    };
    let result = state
        .databases
        .create_database(&user, owner_id, &form.owner_username, &form.suffix, charset)
        .await;
    match result {
        Ok(_) => Redirect::to("/databases").into_response(),
        Err(e) => render_create_error(&state, &user, &csrf, &e.to_string(), &form).await,
    }
}

/// GET /databases/{id} — the database detail page.
pub async fn detail(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    match state.databases.get_database(&user, id).await {
        Ok(db) => {
            let owner = owner_name(&state, db.owner_id()).await;
            let content = html! {
                h1 { (db.name()) }
                (detail_section(&db, &owner, &csrf, user.role().can_manage_databases()))
            };
            state
                .render_shell(&user, &csrf, "/databases", content)
                .await
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "database not found").into_response(),
    }
}

/// POST /databases/{id}/password — rotate the password and return the
/// new plaintext once.
pub async fn change_password(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<PasswordActionForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let result = state.databases.change_password(&user, id).await;
    password_panel_response(result, &csrf, "rotated", Utc::now()).into_response()
}

/// POST /databases/{id}/reveal — return the current plaintext once.
pub async fn reveal(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<PasswordActionForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let result = state.databases.reveal_password(&user, id).await;
    password_panel_response(result, &csrf, "revealed", Utc::now()).into_response()
}

/// DELETE /databases/{id} — delete (with confirmation).
pub async fn delete(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    _csrf: ValidateCsrf,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let databases = state.databases.clone();
    let user_for_action = user.clone();
    let result = async move { databases.delete_database(&user_for_action, id).await };
    action_response(&state, &user, &csrf, async move {
        result.await.map_err(|e| e.to_string())
    })
    .await
}

/// Run a state-changing action, then re-render the list fragment.
async fn action_response<F>(
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
    action: F,
) -> Response
where
    F: std::future::Future<Output = Result<(), String>>,
{
    let error = action.await.err();
    let dbs = state
        .databases
        .list_databases(user)
        .await
        .unwrap_or_default();
    let rows = collect_rows(&dbs, user);
    let fragment = match &error {
        Some(msg) => html! {
            (error_region(msg))
            (list_fragment(&rows, user.role().can_manage_databases(), csrf))
        },
        None => list_fragment(&rows, user.role().can_manage_databases(), csrf),
    };
    (StatusCode::OK, fragment).into_response()
}

/// Build the response for change-password / reveal: the password panel
/// (with the new plaintext when present) and the list fragment below.
fn password_panel_response(
    result: Result<String, openpanel_domain::databases::error::DatabaseError>,
    csrf: &str,
    action: &str,
    now: DateTime<Utc>,
) -> Markup {
    match result {
        Ok(plaintext) => html! {
            section id="password-panel" class="password-panel" {
                div class="alert ok" role="status" { "Password " (action) " at " (format_dt(now)) }
                p { strong { "Plaintext password (shown once):" } }
                pre { code { (plaintext) } }
                p class="muted" { "Copy this value now. The page does not retain it." }
            }
            (list_fragment(&[], true, csrf))
        },
        Err(e) => html! {
            (error_region(&e.to_string()))
            (list_fragment(&[], true, csrf))
        },
    }
}

/// Re-render the create form with an inline error.
async fn render_create_error(
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
    msg: &str,
    form: &CreateDbForm,
) -> Response {
    let safe_form = SafeCreateForm {
        owner_id: &form.owner_id,
        owner_username: &form.owner_username,
        suffix: &form.suffix,
        charset: &form.charset,
    };
    let content = html! {
        h1 { "New database" }
        (create_form(csrf, Some(msg), Some(&safe_form)))
    };
    state
        .render_shell(user, csrf, "/databases", content)
        .await
        .into_response()
}

/// `CreateDbForm` minus the CSRF token. Used when re-rendering after a
/// validation failure so the secret is never echoed.
pub struct SafeCreateForm<'a> {
    /// Owner UUID the database is being created under.
    pub owner_id: &'a str,
    /// Owner's username (used to seed the db_user).
    pub owner_username: &'a str,
    /// Optional name suffix.
    pub suffix: &'a str,
    /// Optional character set.
    pub charset: &'a str,
}

/// Render the `#databases-list` fragment: a table of databases with
/// per-row actions when the caller is allowed to manage databases.
pub fn list_fragment(rows: &[DbRow], can_manage: bool, csrf: &str) -> Markup {
    html! {
        section id="databases-list" {
            @if can_manage {
                a class="btn" href="/databases/new" { "Add database" }
            }
            @if rows.is_empty() {
                @if can_manage {
                    (crate::ui_states::EmptyState::new("No databases yet", "Create your first database to start storing data.")
                        .with_cta("/databases/new", "Add database")
                        .render())
                } @else {
                    (crate::ui_states::EmptyState::new("No databases yet", "No databases are assigned to your account.").render())
                }
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Name" }
                            th { "Owner" }
                            th { "Charset" }
                            th { "Status" }
                            th { "Quota" }
                            th { "Backups" }
                            @if can_manage { th { "Actions" } }
                        }
                    }
                    tbody {
                        @for row in rows {
                            (render_database_row(
                                &DatabaseRowView {
                                    id: row.id,
                                    name: &row.name,
                                    owner: &row.owner,
                                    charset: &row.charset,
                                    status: &row.status,
                                    quota: row.quota.as_deref(),
                                    backup_count: row.backup_count,
                                },
                                can_manage,
                                csrf,
                            ))
                        }
                    }
                }
            }
        }
    }
}

/// Render the create form. `values` re-populates the form after a
/// validation failure; the CSRF token is the only secret and is held
/// by the layout.
pub fn create_form(csrf: &str, error: Option<&str>, values: Option<&SafeCreateForm<'_>>) -> Markup {
    let owner_id = values.map(|v| v.owner_id).unwrap_or("");
    let owner_username = values.map(|v| v.owner_username).unwrap_or("");
    let suffix = values.map(|v| v.suffix).unwrap_or("");
    let charset = values.map(|v| v.charset).unwrap_or("utf8mb4");
    html! {
        @if let Some(msg) = error {
            (error_region(msg))
        }
        form method="post" action="/databases" class="form" {
            (csrf_field(csrf))
            label { "Owner id (UUID)" input type="text" name="owner_id" value=(owner_id) required; }
            label { "Owner username" input type="text" name="owner_username" value=(owner_username) required; }
            label { "Suffix" input type="text" name="suffix" value=(suffix) required; }
            label { "Charset" input type="text" name="charset" value=(charset); }
            button type="submit" { "Create database" }
        }
    }
}

/// Render the database detail section.
pub fn detail_section(db: &Database, owner: &str, csrf: &str, can_manage: bool) -> Markup {
    let _ = csrf;
    html! {
        section class="detail" {
            p { strong { "Name:" } " " (db.name()) }
            p { strong { "Owner:" } " " (owner) }
            p { strong { "DB user:" } " " (db.db_user()) }
            p { strong { "DB host:" } " " (db.db_host()) }
            p { strong { "Engine:" } " " (db.engine().as_str()) }
            p { strong { "Charset:" } " " (db.charset()) }
            p { strong { "Status:" } " " (db.status().as_str()) }
            p { strong { "Created at:" } " " (format_dt(db.created_at())) " by " (db.created_by()) }
            @if can_manage {
                nav class="links" {
                    a href="/databases" { "Back to list" }
                    a href=(format!("/databases/{}/pitr", db.id())) { "Point-in-time recovery" }
                }
            }
        }
    }
}

/// Render an inline error region.
pub fn error_region(message: &str) -> Markup {
    html! {
        div class="alert error" role="alert" { (message) }
    }
}

fn collect_rows(dbs: &[Database], user: &openpanel_domain::User) -> Vec<DbRow> {
    let _ = user;
    dbs.iter()
        .map(|db| DbRow {
            id: db.id(),
            name: db.name().to_string(),
            owner: db.created_by().to_string(),
            charset: db.charset().to_string(),
            status: db.status().as_str().to_string(),
            quota: None,
            backup_count: None,
        })
        .collect()
}

async fn owner_name(state: &WebState, _id: Uuid) -> String {
    let _ = state;
    String::new()
}

fn format_dt(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M UTC").to_string()
}

#[cfg(test)]
mod tests;
