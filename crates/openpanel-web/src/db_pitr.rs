//! Web pages for the `db-pitr` bounded context.
//!
//! A minimal page that lists the available transaction-log range
//! for a database and offers a restore dialog. The dialog POSTs
//! directly to the API endpoint; the page is a thin shell.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use uuid::Uuid;

use crate::router::WebState;

/// Page for a single database's PITR status and binlog range.
pub async fn page(State(state): State<WebState>, Path(database_id): Path<Uuid>) -> Response {
    let range = state.pitr.inspect_range(database_id).await.ok();
    let body: Markup = html! {
        section class="card" {
            h2 { "Database point-in-time recovery" }
            @if let Some(r) = range {
                p { "Earliest: " code { (r.earliest.to_hex()) } }
                p { "Latest: " code { (r.latest.to_hex()) } }
                p { "Window: "
                    code { (r.start.to_rfc3339()) }
                    " to "
                    code { (r.end.to_rfc3339()) }
                }
                @if r.empty {
                    p class="muted" { "No binlog segments have been streamed yet." }
                } @else {
                    form method="post" action={ "/api/v1/backups/databases/" (database_id) "/pitr/restore" } class="form" {
                        label { "Restore to (RFC 3339)" input type="text" name="timestamp" required; }
                        button type="submit" { "Request restore" }
                    }
                }
            } @else {
                p class="muted" { "Database not found." }
            }
        }
    };
    (StatusCode::OK, body).into_response()
}
