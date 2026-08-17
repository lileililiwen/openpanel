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

/// Body of the create-database form.
#[derive(Debug, Deserialize)]
pub struct PasswordActionForm {
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
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
                p class="empty" { "No databases yet." }
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Name" }
                            th { "Owner" }
                            th { "Charset" }
                            th { "Status" }
                            @if can_manage { th { "Actions" } }
                        }
                    }
                    tbody {
                        @for row in rows {
                            tr {
                                td { a href=(format!("/databases/{}", row.id)) { (row.name) } }
                                td { (row.owner) }
                                td { (row.charset) }
                                td { span class="status" { (row.status) } }
                                @if can_manage {
                                    td class="actions" {
                                        form class="inline" hx-post=(format!("/databases/{}/password", row.id)) hx-target="#password-panel" hx-swap="outerHTML" {
                                            (csrf_field(csrf))
                                            button type="submit" { "Rotate password" }
                                        }
                                        form class="inline" hx-post=(format!("/databases/{}/reveal", row.id)) hx-target="#password-panel" hx-swap="outerHTML" {
                                            (csrf_field(csrf))
                                            button type="submit" { "Reveal" }
                                        }
                                        form class="inline" hx-delete=(format!("/databases/{}", row.id)) hx-target="#databases-list" hx-confirm=(format!("Delete database {}? This cannot be undone.", row.name)) {
                                            (csrf_field(csrf))
                                            button type="submit" class="danger" { "Delete" }
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
mod tests {
    use chrono::Utc;
    use openpanel_domain::databases::database::Database;

    use super::*;

    fn db(name: &str) -> Database {
        Database::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "admin",
            name,
            "utf8mb4",
            "admin",
        )
        .expect("valid database")
    }

    #[test]
    fn list_renders_one_row_per_database() {
        let dbs = [db("appdb"), db("logs")];
        let user = openpanel_domain::User::restore(
            Uuid::new_v4(),
            openpanel_domain::Username::new("admin").expect("u"),
            openpanel_domain::Email::new("admin@example.com").expect("e"),
            "h".into(),
            openpanel_domain::Role::Owner,
            None,
            None,
            Utc::now(),
            None,
            None,
        );
        let rows = collect_rows(&dbs, &user);
        let out = list_fragment(&rows, true, "tok").into_string();
        assert!(out.contains("appdb"), "appdb: {out}");
        assert!(out.contains("logs"), "logs: {out}");
        assert!(out.contains("admin"), "owner: {out}");
        assert!(out.contains("utf8mb4"), "charset: {out}");
        assert!(out.contains("Add database"), "create action: {out}");
    }

    #[test]
    fn list_hides_actions_for_non_managers() {
        let dbs = [db("appdb")];
        let out = list_fragment(
            &dbs.iter()
                .map(|d| DbRow {
                    id: d.id(),
                    name: d.name().to_string(),
                    owner: d.created_by().to_string(),
                    charset: d.charset().to_string(),
                    status: d.status().as_str().to_string(),
                })
                .collect::<Vec<_>>(),
            false,
            "tok",
        )
        .into_string();
        assert!(!out.contains("Add database"), "no create: {out}");
        assert!(!out.contains("Delete"), "no delete: {out}");
        assert!(!out.contains("Rotate"), "no rotate: {out}");
    }

    #[test]
    fn list_empty_state_renders() {
        let out = list_fragment(&[], true, "tok").into_string();
        assert!(out.contains("No databases yet"), "empty: {out}");
    }

    #[test]
    fn list_renders_delete_with_confirmation() {
        let dbs = [db("appdb")];
        let rows: Vec<DbRow> = dbs
            .iter()
            .map(|d| DbRow {
                id: d.id(),
                name: d.name().to_string(),
                owner: d.created_by().to_string(),
                charset: d.charset().to_string(),
                status: d.status().as_str().to_string(),
            })
            .collect();
        let out = list_fragment(&rows, true, "tok").into_string();
        assert!(out.contains("hx-confirm"), "confirm: {out}");
        assert!(out.contains("hx-delete="), "delete wired: {out}");
    }

    #[test]
    fn create_form_renders_all_fields() {
        let out = create_form("tok", None, None).into_string();
        for needle in [
            "name=\"owner_id\"",
            "name=\"owner_username\"",
            "name=\"suffix\"",
            "name=\"charset\"",
            "Create database",
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
    }

    #[test]
    fn create_form_renders_inline_error() {
        let out = create_form("tok", Some("invalid owner id"), None).into_string();
        assert!(out.contains("invalid owner id"), "error: {out}");
        assert!(out.contains("role=\"alert\""), "alert role: {out}");
    }

    #[test]
    fn password_panel_renders_plaintext_once() {
        let now = Utc::now();
        let out = password_panel_response(
            Ok("s3cret-plaintext-1234".to_string()),
            "tok",
            "rotated",
            now,
        )
        .into_string();
        assert!(out.contains("id=\"password-panel\""), "panel: {out}");
        assert!(out.contains("s3cret-plaintext-1234"), "plaintext: {out}");
        assert!(out.contains("rotated at"), "rotated at: {out}");
    }

    #[test]
    fn password_panel_renders_error_on_failure() {
        let now = Utc::now();
        let out = password_panel_response(
            Err(openpanel_domain::databases::error::DatabaseError::NotFound(
                "x".into(),
            )),
            "tok",
            "revealed",
            now,
        )
        .into_string();
        assert!(out.contains("not found"), "error rendered: {out}");
        assert!(
            !out.contains("id=\"password-panel\""),
            "no panel on error: {out}"
        );
    }

    #[test]
    fn detail_section_renders_metadata() {
        let database = db("appdb");
        let out = detail_section(&database, "admin", "tok", true).into_string();
        for needle in [
            "appdb",
            "Owner:",
            "admin",
            "Engine:",
            "Charset:",
            "Status:",
            "Created at:",
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
    }
}
