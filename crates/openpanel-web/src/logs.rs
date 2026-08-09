//! Browser log source, entry, traffic, and audit views.

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use maud::html;
use openpanel_app::logs::{LogActor, LogReadQuery};
use serde::Deserialize;
use uuid::Uuid;

use crate::router::{WebState, WebUser};

/// Render the logs and traffic landing page.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let actor = LogActor::new(user.id(), user.role());
    let sources = state.logs.sources(actor).await.unwrap_or_default();
    let traffic = state.logs.traffic(actor).await.unwrap_or_default();
    let content = html! {
        h1 { "Logs & traffic" }
        p { "Bounded, redacted views of registered sources." }
        h2 { "Sources" }
        ul { @for source in sources { li { a href=(format!("/logs/entries?source_id={}", source.id())) { (source.name()) } } } }
        h2 { "Traffic" }
        @if traffic.is_empty() { p { "No retained traffic summaries." } }
    };
    state
        .render_shell(&user, &csrf, "/logs", content)
        .await
        .into_response()
}

#[derive(Debug, Deserialize)]
/// Query fields for the polling entry fragment.
pub struct EntriesQuery {
    source_id: Uuid,
    limit: Option<usize>,
    text: Option<String>,
    cursor: Option<String>,
}

/// Render an escaped HTMX-friendly entry fragment.
pub async fn entries(
    State(state): State<WebState>,
    WebUser(user, _): WebUser,
    Query(input): Query<EntriesQuery>,
) -> Response {
    let actor = LogActor::new(user.id(), user.role());
    let mut query = LogReadQuery::new(input.source_id, input.limit.unwrap_or(100));
    query.text = input.text;
    query.cursor = input.cursor;
    match state.logs.entries(actor, query).await {
        Ok(page) => html! { div id="log-entries" hx-get=(format!("/logs/entries?source_id={}", input.source_id)) hx-trigger="every 5s" { @for entry in page.entries { pre { (entry.text) } } } }.into_response(),
        Err(error) => (axum::http::StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}
