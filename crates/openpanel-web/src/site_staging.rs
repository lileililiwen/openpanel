//! Per-site staging web pages.
//!
//! A minimal page showing the current slot, recent promotions, and
//! a "snapshot" / "promote" / "destroy" set of forms. The forms POST
//! directly to the JSON API endpoints.

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use uuid::Uuid;

use crate::router::{WebState, WebUser};
use crate::site_workspace::TabId;

/// Page for a single site's staging status, recent promotions, and
/// a snapshot/promote/destroy form set.
pub async fn page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(site_id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let slot = state.staging.slot_for(&user, site_id).await.ok().flatten();
    let body: Markup = html! {
        (crate::site_workspace::site_bar(&state, &user, site_id, TabId::Staging).await)
        section class="card" {
            h2 { "Site staging" }
            p { "The staging slot is at "
                code { "staging.<primary_domain>" }
                " and shares the production PHP runtime."
            }
            form method="post" action={ "/api/v1/sites/" (site_id) "/staging/sync" } class="form form-inline" {
                button type="submit" { "Take snapshot" }
            }
            form method="post" action={ "/api/v1/sites/" (site_id) "/staging/promote" } class="form" {
                label { "Snapshot id" input type="number" name="snapshot" min="1" required; }
                label { "Confirmed at (RFC 3339)" input type="text" name="confirmed_at" required; }
                button type="submit" { "Promote" }
            }
        }
        @if let Some(slot)=slot {
            section class="card" {
                h2 { "Mail capture" }
                @if slot.mail_capture() {
                    p { "Outgoing mail from the staging site is captured instead of delivered." }
                } @else {
                    p { "Capture is off: staging mail is delivered normally." }
                }
                form method="post" action=(format!("/sites/{site_id}/staging/mail-capture")) class="form form-inline" {
                    (crate::layout::csrf_field(&csrf))
                    @if slot.mail_capture() {
                        button type="submit" { "Disable capture" }
                    } @else {
                        input type="hidden" name="enabled" value="true";
                        button type="submit" { "Enable capture" }
                    }
                }
                p { a href=(format!("/mail/capture/{site_id}")) { "Open captured inbox" } }
            }
        }
    };
    let path = format!("/sites/{site_id}/staging");
    state
        .render_shell(&user, &csrf, &path, body)
        .await
        .into_response()
}
