//! Users pages: list, create, role change, enable/disable, password reset, and
//! delete — rendered server-side in the shell, wired to `IdentityService`
//! exactly as the API does.
//!
//! All routes are owner-gated. Actions use HTMX swaps (`hx-post`/`hx-delete`
//! targeting `#user-list`) and the shared CSRF token. Delete requires
//! confirmation (`hx-confirm`).
//!
//! The service is the source of truth for authorization: the handler verifies
//! the caller's role before forwarding, surfaces `LastOwner` errors inline, and
//! never echoes plaintext passwords.

use axum::{
    extract::{Form, Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use chrono::{DateTime, Utc};
use maud::{Markup, html};
use openpanel_domain::{IdentityError, Role, User};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    csrf::ValidateCsrf,
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// One row in the users table (all fields pre-resolved for rendering).
pub struct UserRow {
    /// User id, used for action URLs.
    pub id: Uuid,
    /// Login name.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Snake-case role string.
    pub role: String,
    /// `active` or `disabled`.
    pub status: String,
    /// RFC3339 created-at (or empty when unknown).
    pub created: String,
    /// RFC3339 last-login (or `—` when never).
    pub last_login: String,
    /// Whether the row carries the "you" marker (current user).
    pub is_self: bool,
}

/// Body of the create-user form. `_csrf` is validated in the handler because
/// the form extractor owns the request body.
#[derive(Debug, Deserialize)]
pub struct CreateUserForm {
    /// Login name.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Plaintext password (never rendered back).
    pub password: String,
    /// Role string: `owner`, `admin`, or `user`.
    pub role: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the role-change form.
#[derive(Debug, Deserialize)]
pub struct RoleChangeForm {
    /// New role string: `owner`, `admin`, or `user`.
    pub role: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the password-reset form.
#[derive(Debug, Deserialize)]
pub struct PasswordResetForm {
    /// New plaintext password (never rendered back).
    pub password: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// GET /users — the user list page inside the shell (owner only).
pub async fn list(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    if !user.role().can_manage_users() {
        let body = state
            .render_shell(&user, &csrf, "/users", forbidden_page())
            .await;
        return body.into_response();
    }
    let users = state.identity.list_users().await.unwrap_or_default();
    let rows = collect_rows(&users, user.id());
    let content = html! {
        h1 { "Users" }
        (list_fragment(&rows, &csrf))
    };
    state
        .render_shell(&user, &csrf, "/users", content)
        .await
        .into_response()
}

/// GET /users/new — the create form (owner only).
pub async fn new_form(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    if !user.role().can_manage_users() {
        return (StatusCode::FORBIDDEN, "owner only").into_response();
    }
    let content = html! {
        h1 { "New user" }
        (create_form(&csrf, None, None))
    };
    state
        .render_shell(&user, &csrf, "/users", content)
        .await
        .into_response()
}

/// POST /users — create the user; redirect to the list on success or re-render
/// the form with the inline error.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CreateUserForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !user.role().can_manage_users() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let role = match parse_role(&form.role) {
        Some(r) => r,
        None => return render_create_error(&state, &user, &csrf, "invalid role", &form).await,
    };

    match state
        .identity
        .create_user(
            &form.username,
            &form.email,
            &form.password,
            role,
            user.username().as_str(),
        )
        .await
    {
        Ok(_) => Redirect::to("/users").into_response(),
        Err(e) => render_create_error(&state, &user, &csrf, &e.to_string(), &form).await,
    }
}

/// POST /users/{id}/role — change a user's role; HTMX-swap the row.
pub async fn change_role(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<RoleChangeForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !user.role().can_manage_users() {
        return (StatusCode::FORBIDDEN, "owner only").into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let role = match parse_role(&form.role) {
        Some(r) => r,
        None => {
            return action_response(&state, &user, &csrf, async {
                Err(IdentityError::Forbidden)
            })
            .await;
        }
    };
    let actor = user.username().as_str().to_string();
    let identity = state.identity.clone();
    action_response(&state, &user, &csrf, async move {
        identity.change_role(id, role, &actor).await
    })
    .await
}

/// POST /users/{id}/enable — re-enable a disabled user; HTMX-swap the row.
pub async fn enable(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    _csrf: ValidateCsrf,
) -> Response {
    if !user.role().can_manage_users() {
        return (StatusCode::FORBIDDEN, "owner only").into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let actor = user.username().as_str().to_string();
    let identity = state.identity.clone();
    action_response(&state, &user, &csrf, async move {
        identity.enable_user(id, &actor).await
    })
    .await
}

/// POST /users/{id}/disable — disable a user; HTMX-swap the row.
pub async fn disable(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    _csrf: ValidateCsrf,
) -> Response {
    if !user.role().can_manage_users() {
        return (StatusCode::FORBIDDEN, "owner only").into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let actor = user.username().as_str().to_string();
    let identity = state.identity.clone();
    action_response(&state, &user, &csrf, async move {
        identity.disable_user(id, &actor).await
    })
    .await
}

/// POST /users/{id}/password — reset a user's password; HTMX-swap with a
/// success note that never echoes the value.
pub async fn reset_password(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<PasswordResetForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !user.role().can_manage_users() {
        return (StatusCode::FORBIDDEN, "owner only").into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let actor = user.username().as_str().to_string();
    let fragment = match state
        .identity
        .change_password(id, &form.password, &actor)
        .await
    {
        Ok(()) => {
            let users = state.identity.list_users().await.unwrap_or_default();
            let rows = collect_rows(&users, user.id());
            html! {
                div class="alert ok" role="status" { "password updated" }
                (list_fragment(&rows, &csrf))
            }
        }
        Err(e) => {
            let users = state.identity.list_users().await.unwrap_or_default();
            let rows = collect_rows(&users, user.id());
            html! {
                (error_region(&e.to_string()))
                (list_fragment(&rows, &csrf))
            }
        }
    };
    (StatusCode::OK, fragment).into_response()
}

/// DELETE /users/{id} — remove a user; HTMX-swap the list.
pub async fn delete(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    _csrf: ValidateCsrf,
) -> Response {
    if !user.role().can_manage_users() {
        return (StatusCode::FORBIDDEN, "owner only").into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let actor = user.username().as_str().to_string();
    let identity = state.identity.clone();
    action_response(&state, &user, &csrf, async move {
        identity.delete_user(id, &actor).await
    })
    .await
}

/// Run a state-changing action, then re-render the `#user-list` fragment
/// (HTMX swap). On failure, render the inline error followed by the list.
async fn action_response<F>(state: &WebState, user: &User, csrf: &str, action: F) -> Response
where
    F: std::future::Future<Output = Result<(), IdentityError>>,
{
    let error = match action.await {
        Ok(()) => None,
        Err(e) => Some(e.to_string()),
    };
    let users = state.identity.list_users().await.unwrap_or_default();
    let rows = collect_rows(&users, user.id());
    let fragment = match &error {
        Some(msg) => html! {
            (error_region(msg))
            (list_fragment(&rows, csrf))
        },
        None => list_fragment(&rows, csrf),
    };
    (StatusCode::OK, fragment).into_response()
}

/// Render the create form again with an inline error and the previously
/// submitted values.
async fn render_create_error(
    state: &WebState,
    user: &User,
    csrf: &str,
    msg: &str,
    form: &CreateUserForm,
) -> Response {
    let content = html! {
        h1 { "New user" }
        (create_form(csrf, Some(msg), Some(form)))
    };
    state
        .render_shell(user, csrf, "/users", content)
        .await
        .into_response()
}

/// Render the `#user-list` fragment: a table of users with role/status and
/// owner-only actions (create, role change, enable/disable, password reset,
/// delete with confirmation).
pub fn list_fragment(rows: &[UserRow], csrf: &str) -> Markup {
    html! {
        section id="user-list" {
            a class="btn" href="/users/new" { "New user" }
            @if rows.is_empty() {
                (crate::ui_states::EmptyState::new("No users yet", "Create a user to grant them access to the panel.").render())
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Username" }
                            th { "Email" }
                            th { "Role" }
                            th { "Status" }
                            th { "Created" }
                            th { "Last login" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for row in rows {
                            tr {
                                td { (row.username) @if row.is_self { " (you)" } }
                                td { (row.email) }
                                td { span class="role" { (row.role) } }
                                td { span class="status" { (row.status) } }
                                td { (row.created) }
                                td { (row.last_login) }
                                td class="actions" {
                                    @if !row.is_self {
                                        form class="inline" hx-post=(format!("/users/{}/role", row.id)) hx-target="#user-list" {
                                            (csrf_field(csrf))
                                            select name="role" {
                                                @if row.role == "owner" { option value="owner" selected { "owner" } } @else { option value="owner" { "owner" } }
                                                @if row.role == "admin" { option value="admin" selected { "admin" } } @else { option value="admin" { "admin" } }
                                                @if row.role == "user" { option value="user" selected { "user" } } @else { option value="user" { "user" } }
                                            }
                                            button type="submit" { "Set role" }
                                        }
                                        @if row.status == "active" {
                                            form class="inline" hx-post=(format!("/users/{}/disable", row.id)) hx-target="#user-list" {
                                                (csrf_field(csrf))
                                                button type="submit" { "Disable" }
                                            }
                                        } @else {
                                            form class="inline" hx-post=(format!("/users/{}/enable", row.id)) hx-target="#user-list" {
                                                (csrf_field(csrf))
                                                button type="submit" { "Enable" }
                                            }
                                        }
                                        form class="inline" hx-post=(format!("/users/{}/password", row.id)) hx-target="#user-list" hx-on::after-request="this.reset()" {
                                            (csrf_field(csrf))
                                            input type="password" name="password" placeholder="new password" required;
                                            button type="submit" { "Reset" }
                                        }
                                        a class="btn danger" hx-get=(format!("/layer/confirm?action=delete-user&id={}", row.id))
                                            hx-target="#layer-root" href=(format!("/layer/confirm?action=delete-user&id={}", row.id)) {
                                            "Delete"
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

/// Render the create-user form. `values` re-populates the form after a
/// validation failure; the password field is intentionally empty when
/// re-rendering so a plaintext value is never echoed back.
pub fn create_form(csrf: &str, error: Option<&str>, values: Option<&CreateUserForm>) -> Markup {
    let username = values.map(|v| v.username.as_str()).unwrap_or("");
    let email = values.map(|v| v.email.as_str()).unwrap_or("");
    let role = values.map(|v| v.role.as_str()).unwrap_or("user");
    html! {
        @if let Some(msg) = error {
            (error_region(msg))
        }
        form method="post" action="/users" class="form" {
            (csrf_field(csrf))
            label { "Username" input type="text" name="username" value=(username) required; }
            label { "Email" input type="email" name="email" value=(email) required; }
            label { "Password" input type="password" name="password" required; }
            label { "Role"
                select name="role" {
                    @if role == "owner" { option value="owner" selected { "owner" } } @else { option value="owner" { "owner" } }
                    @if role == "admin" { option value="admin" selected { "admin" } } @else { option value="admin" { "admin" } }
                    @if role == "user" { option value="user" selected { "user" } } @else { option value="user" { "user" } }
                }
            }
            button type="submit" { "Create user" }
        }
    }
}

/// Render the inline error/alert region.
pub fn error_region(message: &str) -> Markup {
    html! {
        div class="alert error" role="alert" { (message) }
    }
}

/// Render the forbidden page for non-owners hitting `/users`.
fn forbidden_page() -> Markup {
    html! {
        section class="detail" {
            h1 { "Forbidden" }
            p { "Only the owner can manage users." }
        }
    }
}

/// Resolve `text` to a `Role` (snake-case). Returns `None` for any other
/// value so the caller can render an inline error.
fn parse_role(text: &str) -> Option<Role> {
    match text {
        "owner" => Some(Role::Owner),
        "admin" => Some(Role::Admin),
        "user" => Some(Role::User),
        _ => None,
    }
}

/// Resolve every user into a row ready for rendering. `self_id` marks the
/// caller so the UI can hide destructive actions on their own row.
fn collect_rows(users: &[User], self_id: Uuid) -> Vec<UserRow> {
    let mut rows = Vec::with_capacity(users.len());
    for user in users {
        rows.push(UserRow {
            id: user.id(),
            username: user.username().as_str().to_string(),
            email: user.email().as_str().to_string(),
            role: user.role().as_str().to_string(),
            status: if user.is_disabled() {
                "disabled"
            } else {
                "active"
            }
            .to_string(),
            created: format_dt(user.created_at()),
            last_login: user
                .last_login_at()
                .map(format_dt)
                .unwrap_or_else(|| "—".into()),
            is_self: user.id() == self_id,
        });
    }
    rows
}

fn format_dt(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M UTC").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: Uuid, username: &str, email: &str, role: &str, status: &str) -> UserRow {
        UserRow {
            id,
            username: username.into(),
            email: email.into(),
            role: role.into(),
            status: status.into(),
            created: "2026-08-09 12:00 UTC".into(),
            last_login: "2026-08-09 13:00 UTC".into(),
            is_self: false,
        }
    }

    #[test]
    fn list_renders_one_row_per_user() {
        let rows = vec![
            row(
                Uuid::new_v4(),
                "admin",
                "admin@example.com",
                "owner",
                "active",
            ),
            row(
                Uuid::new_v4(),
                "alice",
                "alice@example.com",
                "user",
                "active",
            ),
        ];
        let out = list_fragment(&rows, "tok").into_string();
        assert!(out.contains("admin"), "admin: {out}");
        assert!(out.contains("alice"), "alice: {out}");
        assert!(out.contains("admin@example.com"), "email 1: {out}");
        assert!(out.contains("alice@example.com"), "email 2: {out}");
        assert!(out.contains("owner"), "role 1: {out}");
        assert!(out.contains("user"), "role 2: {out}");
        assert!(out.contains("active"), "status: {out}");
        assert!(out.contains("New user"), "create action: {out}");
    }

    #[test]
    fn list_marks_self_and_hides_actions() {
        let self_id = Uuid::new_v4();
        let mut me = row(self_id, "admin", "admin@example.com", "owner", "active");
        me.is_self = true;
        let out = list_fragment(&[me], "tok").into_string();
        assert!(out.contains("admin (you)"), "self marker: {out}");
        assert!(
            !out.contains("Delete"),
            "self row has no destructive action: {out}"
        );
    }

    #[test]
    fn list_empty_state() {
        let out = list_fragment(&[], "tok").into_string();
        assert!(out.contains("No users yet"), "empty: {out}");
    }

    #[test]
    fn list_renders_disable_and_enable_per_status() {
        let mut active = row(Uuid::new_v4(), "alice", "a@e.com", "user", "active");
        active.is_self = true; // hide actions to simplify the assertion
        let out = list_fragment(&[active], "tok").into_string();
        // Self row has no actions.
        assert!(!out.contains("Delete"), "self row has no actions: {out}");

        let mut other = row(Uuid::new_v4(), "bob", "b@e.com", "user", "active");
        other.is_self = false;
        let out = list_fragment(&[other], "tok").into_string();
        assert!(out.contains("Disable"), "active row shows Disable: {out}");

        let mut disabled = row(Uuid::new_v4(), "carol", "c@e.com", "user", "disabled");
        disabled.is_self = false;
        let out = list_fragment(&[disabled], "tok").into_string();
        assert!(out.contains("Enable"), "disabled row shows Enable: {out}");
    }

    #[test]
    fn list_renders_delete_via_layer_confirm() {
        let mut other = row(Uuid::new_v4(), "bob", "b@e.com", "user", "active");
        other.is_self = false;
        let id = other.id;
        let out = list_fragment(&[other], "tok").into_string();
        assert!(
            out.contains(&format!(
                "hx-get=\"/layer/confirm?action=delete-user&amp;id={id}\""
            )),
            "delete routes through layer confirm: {out}"
        );
        assert!(!out.contains("hx-confirm"), "no native confirm: {out}");
        assert!(!out.contains("hx-delete="), "no raw delete: {out}");
    }

    #[test]
    fn create_form_renders_all_fields_without_password_value() {
        let form = CreateUserForm {
            username: "alice".into(),
            email: "alice@example.com".into(),
            password: "supersecret-1234".into(),
            role: "admin".into(),
            _csrf: "tok".into(),
        };
        let out = create_form("tok", None, Some(&form)).into_string();
        for needle in [
            "name=\"username\"",
            "name=\"email\"",
            "name=\"password\"",
            "name=\"role\"",
            "value=\"alice\"",
            "value=\"alice@example.com\"",
            "Create user",
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
        // The plaintext password is never rendered back.
        assert!(!out.contains("supersecret-1234"), "password leaked: {out}");
        // Password input has no `value=` attribute (it's a fresh field).
        assert!(
            !out.contains("value=\"supersecret"),
            "password value attribute leaked: {out}"
        );
    }

    #[test]
    fn create_form_renders_inline_error() {
        let out = create_form("tok", Some("username already taken"), None).into_string();
        assert!(out.contains("username already taken"), "error: {out}");
        assert!(out.contains("role=\"alert\""), "alert role: {out}");
    }

    #[test]
    fn parse_role_recognises_known_values() {
        assert_eq!(parse_role("owner"), Some(Role::Owner));
        assert_eq!(parse_role("admin"), Some(Role::Admin));
        assert_eq!(parse_role("user"), Some(Role::User));
        assert_eq!(parse_role("nope"), None);
        assert_eq!(parse_role(""), None);
    }

    #[test]
    fn collect_rows_marks_self_and_status() {
        let me = Uuid::new_v4();
        let email = openpanel_domain::Email::new("self@example.com").expect("email");
        let uname = openpanel_domain::Username::new("self-user").expect("username");
        let pwd = openpanel_domain::Password::hash("abcdefghijkl").expect("password");
        let mut me_user = User::new(me, uname, email, pwd, Role::Owner);
        me_user.disable();
        let rows = collect_rows(&[me_user], me);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_self, "self");
        assert_eq!(rows[0].status, "disabled", "status");
    }
}
