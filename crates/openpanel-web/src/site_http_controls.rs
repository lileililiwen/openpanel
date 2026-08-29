//! Owner-only per-site HTTP-controls editor.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use openpanel_domain::{Role, site_http_controls::SiteHttpControls};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
    site_workspace::TabId,
};

/// Render the controls document editor.
pub async fn page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if user.role() != Role::Owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let controls = match state.site_http_controls.get(&user, id).await {
        Ok(controls) => controls,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    render(&state, &user, &csrf, id, &controls, None).await
}

/// Browser replacement form carrying a strict JSON controls document.
#[derive(Deserialize)]
pub struct SiteHttpForm {
    _csrf: String,
    controls_json: String,
}

/// Replace the complete controls document after CSRF verification.
pub async fn save(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<SiteHttpForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let parsed = match serde_json::from_str::<SiteHttpControls>(&form.controls_json) {
        Ok(parsed) => parsed,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    // The path parameter is authoritative.
    let desired = match rebuild_with_site(parsed, id) {
        Ok(desired) => desired,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    match state.site_http_controls.put(&user, desired).await {
        Ok(saved) => {
            render(
                &state,
                &user,
                &form._csrf,
                id,
                &saved,
                Some("HTTP controls saved"),
            )
            .await
        }
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

fn rebuild_with_site(
    parsed: SiteHttpControls,
    site_id: Uuid,
) -> Result<SiteHttpControls, openpanel_domain::site_http_controls::SiteHttpError> {
    use openpanel_domain::site_http_controls::SiteHttpControlsInput;
    SiteHttpControls::new(
        site_id,
        parsed.version(),
        SiteHttpControlsInput {
            error_pages: parsed.error_pages().to_vec(),
            redirects: parsed.redirects().to_vec(),
            protected_dirs: parsed.protected_dirs().to_vec(),
            hotlink: parsed.hotlink().cloned(),
            ip_rules: parsed.ip_rules().to_vec(),
            mime_overrides: parsed.mime_overrides().to_vec(),
            index_policy: parsed.index_policy().cloned(),
        },
    )
}

async fn render(
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
    id: Uuid,
    controls: &SiteHttpControls,
    notice: Option<&str>,
) -> Response {
    let controls_json = serde_json::to_string_pretty(controls).unwrap_or_else(|_| "{}".to_owned());
    let content = html! {
        (crate::site_workspace::site_bar(state, user, id, TabId::Http).await)
        h1 { "HTTP controls" }
        p { "Error pages, redirects, protected directories, hotlink protection, client-IP rules, MIME overrides, and index policy are compiled into the managed nginx site configuration and validated before activation." }
        @if let Some(notice) = notice { p class="banner banner--ok" { (notice) } }
        form method="post" action=(format!("/sites/{id}/http")) class="form" {
            (csrf_field(csrf))
            label { "Controls (strict JSON)" textarea name="controls_json" rows="24" { (controls_json) } }
            button type="submit" { "Save HTTP controls" }
        }
        h2 { "Sections" }
        ul {
            li { "Error pages: " (controls.error_pages().len()) }
            li { "Redirects: " (controls.redirects().len()) }
            li { "Protected directories: " (controls.protected_dirs().len()) }
            li { "Client-IP rules: " (controls.ip_rules().len()) }
            li { "MIME overrides: " (controls.mime_overrides().len()) }
        }
    };
    state
        .render_shell(user, csrf, "/sites", content)
        .await
        .into_response()
}
