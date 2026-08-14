//! Per-site collaborator web page.
//!
//! Renders the list of collaborators granted on a site plus an
//! invite form with scope toggles. CSRF is enforced by the
//! shared `ValidateCsrf` extractor.

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use uuid::Uuid;

use crate::router::WebState;

const COLLAB_UI_HEADER: &str = "collaborators-ui-pending";

/// Render the collaborators page for a given site.
pub async fn page(State(state): State<WebState>, Path(site_id): Path<Uuid>) -> Response {
    let grants = state
        .collaborators
        .grants_for_site(site_id)
        .await
        .unwrap_or_default();
    render(&site_id, &grants).into_response()
}

fn render(site_id: &Uuid, grants: &[openpanel_domain::SiteGrant]) -> Markup {
    let stub = COLLAB_UI_HEADER;
    html! {
        div class="collaborators" data-stub=(stub) {
            h1 { "Collaborators" }
            p { (format!("site: {site_id}")) }
            @for g in grants {
                p { (format!("{} -> {}", g.collaborator_id, g.permissions.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(","))) }
            }
            form method="post" action={ "/api/v1/sites/" (site_id) "/collaborators" } class="form form-inline-row" {
                label { "Email" input name="email" type="email" placeholder="email" required; }
                label { "Scopes" input name="scopes" type="text" placeholder="file,database,mail,cron" required; }
                button type="submit" { "Invite" }
            }
        }
    }
}
