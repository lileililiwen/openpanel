//! Browser service inventory and confirmed lifecycle form.
use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use openpanel_domain::{Role, system_services::ServiceAction};
use serde::Deserialize;

use crate::router::{WebState, WebUser};
/// Render registered service state.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let services = state.system_services.inventory().await.unwrap_or_default();
    let content = html! {h1{"System services"}ul{@for service in services{li{(service.descriptor.display_name()) " — " (service.status.active_state)}}}};
    state
        .render_shell(&user, &csrf, "/services", content)
        .await
        .into_response()
}
#[derive(Default, Deserialize)]
#[serde(default)]
/// Browser lifecycle fields.
pub struct ActionForm {
    _csrf: String,
    action: String,
    confirmed: bool,
}
/// Validate CSRF and perform a typed action.
pub async fn action(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<String>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let action = match form.action.as_str() {
        "start" => ServiceAction::Start,
        "stop" => ServiceAction::Stop,
        "restart" => ServiceAction::Restart,
        "reload" => ServiceAction::Reload,
        "enable" => ServiceAction::Enable,
        "disable" => ServiceAction::Disable,
        _ => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    match state
        .system_services
        .perform(user.id(), user.role(), &id, action, form.confirmed)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(openpanel_app::system_services::ServiceManagerError::Forbidden)
            if !matches!(user.role(), Role::Owner) =>
        {
            StatusCode::FORBIDDEN.into_response()
        }
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
