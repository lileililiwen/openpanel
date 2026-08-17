//! Plugin extension framework web page: installed-plugins list with
//! enable / disable / uninstall forms (CSRF-protected via the
//! session middleware; the forms POST to the JSON API).

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};

use crate::router::WebState;

/// Plugins index page.
pub async fn page(State(_state): State<WebState>) -> Response {
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
    (StatusCode::OK, body).into_response()
}
