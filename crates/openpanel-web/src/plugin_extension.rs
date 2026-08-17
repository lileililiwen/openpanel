//! Plugin extension framework web page: installed-plugins list with
//! enable / disable / uninstall forms (CSRF-protected via the
//! session middleware; the forms POST to the JSON API).

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use openpanel_domain::Role;

use crate::router::{WebState, WebUser};

/// Plugins index page.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    if user.role() != Role::Owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let body: Markup = html! {
        section class="card" {
            h2 { "Plugins" }
            p { "Signed manifests, capability-gated. Installed plugins are listed "
                "below; lifecycle mutations POST to the JSON API." }
            form method="post" action="/api/v1/plugins" class="form form-grid" {
                label { "Manifest JSON"
                    textarea name="manifest" rows="10"
                             placeholder="{\"id\":\"com.example.demo\",...}" required;
                }
                div class="form-actions" {
                    button type="submit" { "Install plugin" }
                }
            }
        }
        section class="card" {
            h2 { "Lifecycle" }
            p { "Use the CLI for enable / disable / uninstall: "
                code { "openpanel plugin list" } ", "
                code { "openpanel plugin enable --id <id>" } ", "
                code { "openpanel plugin disable --id <id>" } ", "
                code { "openpanel plugin uninstall --id <id>" } "."
            }
        }
    };
    let csrf = state.csrf.token_for(session.id());
    state
        .render_shell(&user, &csrf, "/plugins", body)
        .await
        .into_response()
}
