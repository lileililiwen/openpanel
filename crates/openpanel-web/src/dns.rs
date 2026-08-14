//! DNS provider and zone browser pages.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use serde::Deserialize;
use uuid::Uuid;

use crate::router::{WebState, WebUser};

/// Render secret-free provider and zone inventory.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let providers = state.dns.accounts().await.unwrap_or_default();
    let zones = state.dns.zones().await.unwrap_or_default();
    let content = html! {
        h1 { "DNS zones" }
        h2 { "Provider accounts" }
        ul {
            @for provider in providers {
                li {
                    (provider.name) " (" (provider.kind) ")"
                    form method="post" action=(format!("/dns/providers/{}/sync", provider.id)) class="form form-inline" {
                        input type="hidden" name="_csrf" value=(csrf);
                        button { "Synchronize" }
                    }
                }
            }
        }
        form method="post" action="/dns/providers" class="form" {
            input type="hidden" name="_csrf" value=(csrf);
            label { "Provider kind" input name="kind" placeholder="cloudflare"; }
            label { "Account name" input name="name" placeholder="Account name"; }
            label { "API credential" input type="password" name="credential"; }
            button { "Add provider" }
        }
        h2 { "Zones" }
        ul {
            @for zone in zones {
                li { a href=(format!("/dns/zones/{}", zone.id)) { (zone.name.as_str()) } }
            }
        }
    };
    state
        .render_shell(&user, &csrf, "/dns", content)
        .await
        .into_response()
}
#[derive(Default, Deserialize)]
#[serde(default)]
/// Protected provider-create form.
pub struct ProviderForm {
    _csrf: String,
    kind: String,
    name: String,
    credential: String,
}
/// Validate CSRF and create a provider without reflecting credentials.
pub async fn create_provider(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<ProviderForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .dns
        .create_account(
            user.id(),
            user.role(),
            &form.kind,
            &form.name,
            &form.credential,
        )
        .await
    {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(openpanel_app::dns::DnsServiceError::Forbidden) => {
            StatusCode::FORBIDDEN.into_response()
        }
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
#[derive(Default, Deserialize)]
#[serde(default)]
/// CSRF-only provider action form.
pub struct ActionForm {
    _csrf: String,
}
#[derive(Default, Deserialize)]
#[serde(default)]
/// Credential rotation form.
pub struct RotateForm {
    _csrf: String,
    credential: String,
}
fn csrf(state: &WebState, session: &openpanel_domain::Session, token: &str) -> bool {
    state.csrf.verify(session.id(), token)
}
/// Retest stored provider credentials.
pub async fn test_provider(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !csrf(&state, &session, &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state.dns.test_account(user.id(), user.role(), id).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
/// Rotate provider credentials without reflecting them.
pub async fn rotate_provider(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<RotateForm>,
) -> Response {
    if !csrf(&state, &session, &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .dns
        .rotate_account(user.id(), user.role(), id, &form.credential)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
/// Disable provider automation.
pub async fn disable_provider(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !csrf(&state, &session, &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .dns
        .set_account_enabled(user.id(), user.role(), id, false)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
/// Delete local provider metadata only.
pub async fn delete_provider(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !csrf(&state, &session, &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state.dns.delete_account(user.id(), user.role(), id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
/// Synchronize accessible zones and remote records.
pub async fn sync_provider(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !csrf(&state, &session, &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state.dns.sync(user.id(), user.role(), id).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

/// Render one zone and its synchronized records.
pub async fn zone_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let zone = match state
        .dns
        .zones()
        .await
        .ok()
        .and_then(|zones| zones.into_iter().find(|zone| zone.id == id))
    {
        Some(zone) => zone,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let records = state.dns.records(id).await.unwrap_or_default();
    let content = html! {
        h1 { (zone.name.as_str()) }
        ul {
            @for record in records {
                li {
                    (record.name.as_str()) " " (record.data.to_string())
                    form method="post" action=(format!("/dns/zones/{id}/records/{}/update", record.remote_id)) class="form form-inline-row" {
                        input type="hidden" name="_csrf" value=(csrf);
                        input type="hidden" name="name" value=(record.name.as_str());
                        input type="hidden" name="kind" value=(record_kind_name(record.data.kind()));
                        input name="value" value=(record.data.to_string());
                        input name="ttl" value=(record.ttl);
                        input type="hidden" name="expected_version" value=(record.remote_version.as_str());
                        button { "Update" }
                    }
                    form method="post" action=(format!("/dns/zones/{id}/records/{}/delete", record.remote_id)) class="form form-inline" {
                        input type="hidden" name="_csrf" value=(csrf);
                        input type="hidden" name="expected_version" value=(record.remote_version.as_str());
                        button { "Delete" }
                    }
                }
            }
        }
        form method="post" action=(format!("/dns/zones/{id}/records")) class="form form-grid" {
            input type="hidden" name="_csrf" value=(csrf);
            input type="hidden" name="expected_version" value=(zone.remote_version.as_str());
            label { "Name" input name="name"; }
            label { "Kind" input name="kind"; }
            label { "Value" input name="value"; }
            label { "TTL" input name="ttl" value="300"; }
            button { "Add record" }
        }
        form method="post" action=(format!("/dns/zones/{id}/check")) class="form form-inline" {
            input type="hidden" name="_csrf" value=(csrf);
            button { "Check propagation" }
        }
    };
    state
        .render_shell(&user, &csrf, "/dns", content)
        .await
        .into_response()
}

#[derive(Default, Deserialize)]
#[serde(default)]
/// Browser record mutation fields.
pub struct RecordForm {
    _csrf: String,
    name: String,
    kind: String,
    value: String,
    ttl: u32,
    expected_version: String,
}
fn record_kind(value: &str) -> Option<openpanel_domain::dns::RecordKind> {
    use openpanel_domain::dns::RecordKind;
    match value.to_ascii_uppercase().as_str() {
        "A" => Some(RecordKind::A),
        "AAAA" => Some(RecordKind::Aaaa),
        "CNAME" => Some(RecordKind::Cname),
        "TXT" => Some(RecordKind::Txt),
        "MX" => Some(RecordKind::Mx),
        "CAA" => Some(RecordKind::Caa),
        "NS" => Some(RecordKind::Ns),
        "SRV" => Some(RecordKind::Srv),
        _ => None,
    }
}
fn record_kind_name(kind: openpanel_domain::dns::RecordKind) -> &'static str {
    use openpanel_domain::dns::RecordKind;
    match kind {
        RecordKind::A => "A",
        RecordKind::Aaaa => "AAAA",
        RecordKind::Cname => "CNAME",
        RecordKind::Txt => "TXT",
        RecordKind::Mx => "MX",
        RecordKind::Caa => "CAA",
        RecordKind::Ns => "NS",
        RecordKind::Srv => "SRV",
    }
}
/// Create one validated provider record.
pub async fn create_record(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<RecordForm>,
) -> Response {
    if !csrf(&state, &session, &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(kind) = record_kind(&form.kind) else {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    match state
        .dns
        .create_record(
            user.id(),
            user.role(),
            id,
            &form.name,
            kind,
            &form.value,
            form.ttl,
            &form.expected_version,
        )
        .await
    {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
/// Update one provider record using its observed version.
pub async fn update_record(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path((id, record_id)): Path<(Uuid, String)>,
    Form(form): Form<RecordForm>,
) -> Response {
    if !csrf(&state, &session, &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(kind) = record_kind(&form.kind) else {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    match state
        .dns
        .update_record(
            user.id(),
            user.role(),
            id,
            &record_id,
            &form.name,
            kind,
            &form.value,
            form.ttl,
            &form.expected_version,
        )
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
#[derive(Default, Deserialize)]
#[serde(default)]
/// Browser record-delete fields.
pub struct DeleteRecordForm {
    _csrf: String,
    expected_version: String,
}
/// Delete exactly one provider record.
pub async fn delete_record(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path((id, record_id)): Path<(Uuid, String)>,
    Form(form): Form<DeleteRecordForm>,
) -> Response {
    if !csrf(&state, &session, &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .dns
        .delete_record(
            user.id(),
            user.role(),
            id,
            &record_id,
            &form.expected_version,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
/// Run a bounded propagation check.
pub async fn check_zone(
    State(state): State<WebState>,
    WebUser(_user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !csrf(&state, &session, &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state.dns.check(id).await {
        Ok(result) => (StatusCode::OK, result.status).into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
