//! Container runtime web pages: the per-user quota editor and the
//! registry-credential lifecycle. Both are Owner-only and CSRF
//! protected on every state-changing POST.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use openpanel_domain::{ContainerQuota, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// Render the per-user quota editor for the caller.
pub async fn quota_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
) -> Response {
    if user.role() != Role::Owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    match state.container_runtime.get_quota(&user, user.id()).await {
        Ok(effective) => {
            let content = html! {
                h1 { "Container quota" }
                p class="banner banner--warning" {
                    "The docker service reads this quota before every container create; a 5th concurrent or 9th total container is refused. Plan caps tighten the axes shown below."
                }
                form method="post" action="/container/quota" class="form" {
                    (csrf_field(&csrf))
                    label { "max_concurrent" input type="number" name="max_concurrent" min="0" value=(effective.quota.max_concurrent); }
                    label { "max_total" input type="number" name="max_total" min="0" value=(effective.quota.max_total); }
                    label { "cpu_pct_max" input type="number" name="cpu_pct_max" min="0" max="100" value=(effective.quota.cpu_pct_max); }
                    label { "memory_bytes_max" input type="number" name="memory_bytes_max" min="0" value=(effective.quota.memory_bytes_max); }
                    label { "egress_bytes_per_month" input type="number" name="egress_bytes_per_month" min="0" value=(effective.quota.egress_bytes_per_month); }
                    button type="submit" { "Save quota" }
                }
                h2 { "Plan overrides" }
                @if effective.plan_overrides.is_empty() {
                    p { "No plan caps are tightening this quota." }
                } @else {
                    ul { @for axis in &effective.plan_overrides { li { (format!("{axis:?}")) } } }
                }
            };
            state
                .render_shell(&user, &csrf, "/container/quota", content)
                .await
                .into_response()
        }
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

/// Browser quota-update form. Empty fields preserve the current value.
#[derive(Deserialize, Default)]
pub struct QuotaForm {
    _csrf: String,
    max_concurrent: Option<u32>,
    max_total: Option<u32>,
    cpu_pct_max: Option<u8>,
    memory_bytes_max: Option<u64>,
    egress_bytes_per_month: Option<u64>,
}

/// Validate CSRF and save the caller's quota.
pub async fn quota_update(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<QuotaForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let update = openpanel_app::container_runtime::QuotaUpdate {
        max_concurrent: form.max_concurrent,
        max_total: form.max_total,
        cpu_pct_max: form.cpu_pct_max,
        memory_bytes_max: form.memory_bytes_max,
        egress_bytes_per_month: form.egress_bytes_per_month,
    };
    match state
        .container_runtime
        .set_quota(&user, user.id(), update)
        .await
    {
        Ok(effective) => render_quota(&state, &user, &form._csrf, Some(&effective)).await,
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

/// Render the registry credential list and add form.
pub async fn registry_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
) -> Response {
    if user.role() != Role::Owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let creds = state
        .container_runtime
        .list_registry_credentials(&user, user.id())
        .await
        .unwrap_or_default();
    let content = html! {
        h1 { "Registry credentials" }
        p class="banner banner--warning" { "Passwords are encrypted at rest and returned in plaintext exactly once on create." }
        form method="post" action="/registry/credentials" class="form" {
            (csrf_field(&csrf))
            label { "Registry" input type="text" name="registry" required; }
            label { "Username" input type="text" name="username" required; }
            label { "Password" input type="password" name="password" required; }
            button type="submit" { "Add credential" }
        }
        h2 { "Credentials" }
        @if creds.is_empty() { p { "No registry credentials yet." } } @else {
            ul { @for cred in creds {
                li {
                    (format!("{} / {}", cred.registry, cred.username)) " "
                    form method="post" action=(format!("/registry/credentials/{}/remove", cred.id)) class="form form-inline" {
                        (csrf_field(&csrf)) button type="submit" { "Remove" }
                    }
                }
            } }
        }
    };
    state
        .render_shell(&user, &csrf, "/registry/credentials", content)
        .await
        .into_response()
}

/// Browser credential-add form.
#[derive(Deserialize)]
pub struct CredentialForm {
    _csrf: String,
    registry: String,
    username: String,
    password: String,
}

/// Validate CSRF and store a new credential; plaintext returned once.
pub async fn registry_create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CredentialForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .container_runtime
        .create_registry_credential(&user, form.registry, form.username, form.password)
        .await
    {
        Ok(_) => render_registry(&state, &user, &form._csrf).await,
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

/// CSRF-only form used by the credential remove action.
#[derive(Deserialize)]
pub struct RemoveForm {
    _csrf: String,
}

/// Validate CSRF and delete one credential.
pub async fn registry_remove(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<RemoveForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .container_runtime
        .delete_registry_credential(&user, id)
        .await
    {
        Ok(_) => render_registry(&state, &user, &form._csrf).await,
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

async fn render_quota(
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
    effective: Option<&openpanel_domain::EffectiveQuota>,
) -> Response {
    let effective = match effective {
        Some(value) => value.clone(),
        None => state
            .container_runtime
            .get_quota(user, user.id())
            .await
            .unwrap_or(openpanel_domain::EffectiveQuota {
                quota: ContainerQuota::default_for(user.id()),
                plan_overrides: Vec::new(),
            }),
    };
    let content = html! {
        h1 { "Container quota" }
        @if effective.plan_overrides.is_empty() {
            p { "Saved." }
        }
        form method="post" action="/container/quota" class="form" {
            (csrf_field(csrf))
            label { "max_concurrent" input type="number" name="max_concurrent" min="0" value=(effective.quota.max_concurrent); }
            label { "max_total" input type="number" name="max_total" min="0" value=(effective.quota.max_total); }
            label { "cpu_pct_max" input type="number" name="cpu_pct_max" min="0" max="100" value=(effective.quota.cpu_pct_max); }
            label { "memory_bytes_max" input type="number" name="memory_bytes_max" min="0" value=(effective.quota.memory_bytes_max); }
            label { "egress_bytes_per_month" input type="number" name="egress_bytes_per_month" min="0" value=(effective.quota.egress_bytes_per_month); }
            button type="submit" { "Save quota" }
        }
    };
    state
        .render_shell(user, csrf, "/container/quota", content)
        .await
        .into_response()
}

async fn render_registry(state: &WebState, user: &openpanel_domain::User, csrf: &str) -> Response {
    let creds = state
        .container_runtime
        .list_registry_credentials(user, user.id())
        .await
        .unwrap_or_default();
    let content = html! {
        h1 { "Registry credentials" }
        form method="post" action="/registry/credentials" class="form" {
            (csrf_field(csrf))
            label { "Registry" input type="text" name="registry" required; }
            label { "Username" input type="text" name="username" required; }
            label { "Password" input type="password" name="password" required; }
            button type="submit" { "Add credential" }
        }
        h2 { "Credentials" }
        @if creds.is_empty() { p { "No registry credentials yet." } } @else {
            ul { @for cred in creds {
                li {
                    (format!("{} / {}", cred.registry, cred.username)) " "
                    form method="post" action=(format!("/registry/credentials/{}/remove", cred.id)) class="form form-inline" {
                        (csrf_field(csrf)) button type="submit" { "Remove" }
                    }
                }
            } }
        }
    };
    state
        .render_shell(user, csrf, "/registry/credentials", content)
        .await
        .into_response()
}
