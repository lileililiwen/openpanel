//! Owner-only Software Center pages: storefront, detail, and install wizard.
//!
//! The page is rendered on the server; search, filter, sort, and refresh
//! all work as plain form GETs so the experience degrades gracefully
//! without JavaScript. HTMX is used for the install wizard and job
//! progress panel for a snappier feel when available.
#![allow(missing_docs)]

use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use openpanel_app::software_center::{
    CatalogQuery, CatalogSearchPage, CompatibilityHost, StorefrontEntry,
};
use openpanel_domain::software_center::CatalogHit;
use serde::Deserialize;

use crate::router::{WebState, WebUser};

/// Storefront page: category tabs, search, card grid, diagnostics strip,
/// and job-progress panel.
pub async fn page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Query(params): Query<StorefrontQuery>,
) -> Response {
    let query = build_query(&params);
    let page_result = state
        .software_center
        .search(user.role(), query.clone())
        .await;
    let diagnostics = state
        .software_center
        .catalog_diagnostics(user.role())
        .await
        .ok();
    let jobs = state
        .software_center
        .jobs(user.role())
        .await
        .unwrap_or_default();
    let csrf = state.csrf.token_for(session.id());
    let content = match page_result {
        Ok(page) => storefront_content(&page, &params, diagnostics.as_ref(), &jobs, &csrf),
        Err(_) => error_content("Failed to load the Software Center catalog."),
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

fn build_query(params: &StorefrontQuery) -> CatalogQuery {
    let mut query = CatalogQuery {
        text: params.q.clone(),
        page: params.page.unwrap_or(0),
        page_size: params.page_size.unwrap_or(60),
        ..CatalogQuery::default()
    };
    if let Some(category) = &params.category
        && let Ok(value) = category.parse()
    {
        query.categories.push(value);
    }
    if let Some(tag) = &params.tag
        && let Ok(value) = openpanel_domain::software_center::Tag::new(tag)
    {
        query.tags.push(value);
    }
    if params.installed_only.is_some_and(|value| value) {
        query.installed_only = true;
    }
    if params.update_available_only.is_some_and(|value| value) {
        query.update_available_only = true;
    }
    if let Some(sort) = &params.sort {
        query.sort = match sort.as_str() {
            "recent" => openpanel_domain::software_center::CatalogSort::Recent,
            "size" => openpanel_domain::software_center::CatalogSort::Size,
            _ => openpanel_domain::software_center::CatalogSort::Name,
        };
    }
    query
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct StorefrontQuery {
    pub q: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub category: Option<String>,
    pub tag: Option<String>,
    pub installed_only: Option<bool>,
    pub update_available_only: Option<bool>,
    pub sort: Option<String>,
}

fn storefront_content(
    page: &CatalogSearchPage,
    params: &StorefrontQuery,
    diagnostics: Option<&openpanel_app::software_center::CatalogDiagnostics>,
    jobs: &[openpanel_app::software_center::SoftwareJobView],
    csrf: &str,
) -> Markup {
    let active_category = params.category.as_deref().unwrap_or("all");
    html! {
        div class="storefront" {
            header class="storefront__header" {
                h1 { "Software Center" }
                p { "Curated system components and one-click web applications." }
            }

            section class="storefront__diagnostics" aria-label="Catalog diagnostics" {
                @if let Some(diag) = diagnostics {
                    span class="badge" { (diag.source_id) }
                    span { "Source: " (diag.source_url) }
                    span { "Entries: " (diag.entry_count) }
                    span { "Last refresh: " (diag.activated_at) }
                    @if diag.stale {
                        span class="badge badge--warning" { "stale" }
                    }
                    form method="post" action="/software/refresh" {
                        input type="hidden" name="_csrf" value=(csrf);
                        button class="button" { "Refresh catalog" }
                    }
                } @else {
                    span { "Catalog unavailable" }
                }
            }

            nav class="storefront__tabs" aria-label="Categories" {
                a class=(if active_category == "all" { "tab tab--active" } else { "tab" })
                  href="/software" { "All" }
                @for cat in openpanel_domain::software_center::Category::all() {
                    @let slug = cat.slug();
                    a class=(if active_category == slug { "tab tab--active" } else { "tab" })
                      href={"/software?category=" (slug)} { (cat.label()) }
                }
            }

            form method="get" action="/software" class="storefront__search" {
                input type="search" name="q" value=(params.q.clone().unwrap_or_default()) placeholder="Search the catalog…";
                @if let Some(category) = &params.category {
                    input type="hidden" name="category" value=(category);
                }
                @if let Some(tag) = &params.tag {
                    input type="hidden" name="tag" value=(tag);
                }
                button class="button" { "Search" }
            }

            div class="storefront__filters" {
                label class="toggle" {
                    input type="checkbox" name="installed_only" value="1"
                      checked[params.installed_only.unwrap_or(false)] hx-get="/software"
                      hx-trigger="change" hx-target="body" hx-swap="none"
                      hx-vals={"category:" (params.category.clone().unwrap_or_default())
                               ",q:" (params.q.clone().unwrap_or_default())
                               ",tag:" (params.tag.clone().unwrap_or_default())};
                    span { "Installed only" }
                }
                label class="toggle" {
                    input type="checkbox" name="update_available_only" value="1"
                      checked[params.update_available_only.unwrap_or(false)];
                    span { "Update available" }
                }
                label class="select" {
                    span { "Sort" }
                    select name="sort" {
                        option value="name" selected[params.sort.as_deref() != Some("recent") && params.sort.as_deref() != Some("size")] { "Name" }
                        option value="recent" selected[params.sort.as_deref() == Some("recent")] { "Recent" }
                        option value="size" selected[params.sort.as_deref() == Some("size")] { "Size" }
                    }
                }
            }

            @if page.hits.is_empty() {
                section class="empty-state" {
                    h2 { "No matches" }
                    p { "The active catalog has no entry that matches the current filters." }
                    a class="button" href="/software" { "Clear filters" }
                }
            } @else {
                div class="storefront__grid" {
                    @for hit in &page.hits {
                        (card(hit, csrf))
                    }
                }
            }

            @if !jobs.is_empty() {
                section class="storefront__jobs" aria-label="Recent jobs" {
                    h2 { "Recent jobs" }
                    ul {
                        @for job in jobs.iter().take(8) {
                            li {
                                span class="badge" { (job.state) }
                                span { (job.id) }
                                @if matches!(job.state.as_str(), "queued" | "running" | "validating") {
                                    form method="post" action={"/software/jobs/" (job.id) "/cancel"} {
                                        input type="hidden" name="_csrf" value=(csrf);
                                        button class="button button--ghost" { "Cancel" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn card(hit: &CatalogHit, csrf: &str) -> Markup {
    let state = hit.install_state.clone();
    let state_label = match state.as_str() {
        "panel_managed" => "Installed",
        "externally_managed" => "External",
        "available" => "Available",
        "unsupported" => "Unsupported",
        _ => "Unknown",
    };
    let state_class = match state.as_str() {
        "panel_managed" => "card__status card__status--ok",
        "externally_managed" => "card__status card__status--external",
        "available" => "card__status",
        "unsupported" => "card__status card__status--warn",
        _ => "card__status",
    };
    let action = action_for_state(&state, &hit.id, csrf);
    html! {
        article class="card" {
            div class="card__icon" aria-hidden="true" { (category_glyph(hit.category.slug())) }
            header class="card__title" {
                h3 { a href={"/software/entries/" (hit.id)} { (hit.name) } }
                span class="card__meta" { (hit.latest_version) " · " (hit.license) }
            }
            p class="card__description" { (hit.description) }
            div class="card__footer" {
                span class=(state_class) { (state_label) }
                (action)
            }
        }
    }
}

fn action_for_state(state: &str, id: &str, csrf: &str) -> Markup {
    match state {
        "panel_managed" => html! {
            form method="post" action={"/software/components/" (id) "/update/preview"} {
                input type="hidden" name="_csrf" value=(csrf);
                button class="button button--ghost" { "Update" }
            }
            form method="post" action={"/software/components/" (id) "/remove/preview"} {
                input type="hidden" name="_csrf" value=(csrf);
                button class="button button--danger" { "Remove" }
            }
        },
        "externally_managed" => html! {
            form method="post" action={"/software/components/" (id) "/adopt/preview"} {
                input type="hidden" name="_csrf" value=(csrf);
                button class="button" { "Adopt" }
            }
        },
        "available" => html! {
            form method="post" action={"/software/components/" (id) "/preview"} {
                input type="hidden" name="_csrf" value=(csrf);
                button class="button" { "Install" }
            }
        },
        _ => html! {},
    }
}

fn category_glyph(slug: &str) -> Markup {
    let symbol = match slug {
        "one-click" => "📦",
        "runtime" => "🐘",
        "database" => "🗄",
        "cache" => "⚡",
        "web-server" => "🌐",
        "mail" => "✉",
        "tools" => "🔧",
        "security" => "🛡",
        _ => "•",
    };
    html! { span class="card__icon-glyph" { (symbol) } }
}

fn error_content(message: &str) -> Markup {
    html! {
        div class="error-state" {
            h1 { "Software Center unavailable" }
            p { (message) }
        }
    }
}

// ---------------------------------------------------------------------------
// Refresh route.
// ---------------------------------------------------------------------------

/// Trigger a manual catalog refresh and redirect back to the storefront.
pub async fn refresh(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CsrfForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let _ = state
        .software_center
        .refresh_catalog(user.id(), user.role())
        .await;
    axum::response::Redirect::to("/software").into_response()
}

#[derive(Deserialize)]
pub struct CsrfForm {
    pub _csrf: String,
}

// ---------------------------------------------------------------------------
// Detail page.
// ---------------------------------------------------------------------------

/// Single-entry detail page with Overview / Versions / Changelog /
/// Dependencies / Source tabs.
pub async fn entry(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<String>,
) -> Response {
    let entry = match state.software_center.entry(user.role(), &id).await {
        Ok(Some(value)) => value,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = detail_content(&entry, &csrf);
    state
        .render_shell(&user, &csrf, &format!("/software/entries/{id}"), content)
        .await
        .into_response()
}

fn detail_content(entry: &StorefrontEntry, csrf: &str) -> Markup {
    let install_action = match entry.install_state.as_str() {
        "available" => Some(html! {
            form method="post" action={"/software/components/" (entry.id) "/preview"} {
                input type="hidden" name="_csrf" value=(csrf);
                button class="button" { "Install" }
            }
        }),
        "externally_managed" => Some(html! {
            form method="post" action={"/software/components/" (entry.id) "/adopt/preview"} {
                input type="hidden" name="_csrf" value=(csrf);
                button class="button" { "Adopt" }
            }
        }),
        "panel_managed" => Some(html! {
            div class="detail__actions" {
                form method="post" action={"/software/components/" (entry.id) "/update/preview"} {
                    input type="hidden" name="_csrf" value=(csrf);
                    button class="button" { "Update" }
                }
                form method="post" action={"/software/components/" (entry.id) "/remove/preview"} {
                    input type="hidden" name="_csrf" value=(csrf);
                    button class="button button--danger" { "Remove" }
                }
            }
        }),
        _ => None,
    };
    html! {
        article class="storefront__detail" {
            header class="detail__header" {
                h1 { (entry.name) }
                span class="badge" { (entry.category.label()) }
                span class="badge" { (entry.kind.as_str()) }
                span class="badge" { (entry.license) }
            }
            p class="detail__lead" { (entry.description) }
            @if let Some(action) = install_action {
                div class="detail__action" { (action) }
            }
            section class="detail__tabs" {
                article class="detail__pane" id="overview" {
                    h2 { "Overview" }
                    p { (entry.long_description) }
                    dl class="detail__metadata" {
                        dt { "Developer" } dd { (entry.developer) }
                        dt { "Homepage" } dd { a href=(entry.homepage) { (entry.homepage) } }
                        dt { "Latest version" } dd { (entry.latest_version) }
                        @if let Some(installed) = &entry.installed_version {
                            dt { "Installed version" } dd { (installed) }
                        }
                    }
                }
                article class="detail__pane" id="versions" {
                    h2 { "Versions" }
                    table class="detail__table" {
                        thead {
                            tr { th { "Version" } th { "Size" } th { "Released" } th { "PHP" } }
                        }
                        tbody {
                            @for version in &entry.versions {
                                tr {
                                    td {
                                        strong { (version.version) }
                                        @if version.is_latest { span class="badge" { "latest" } }
                                    }
                                    td { (format_bytes(version.size_bytes)) }
                                    td { (version.released_at.clone().unwrap_or_else(|| "—".to_owned())) }
                                    td { (version.supports_php.join(", ")) }
                                }
                            }
                        }
                    }
                }
                article class="detail__pane" id="dependencies" {
                    h2 { "Dependencies" }
                    @if entry.dependencies.is_empty() {
                        p { "No declared dependencies." }
                    } @else {
                        ul { @for dep in &entry.dependencies { li { a href={"/software/entries/" (dep)} { (dep) } } } }
                    }
                    h3 { "Conflicts" }
                    @if entry.conflicts.is_empty() {
                        p { "No declared conflicts." }
                    } @else {
                        ul { @for conflict in &entry.conflicts { li { (conflict) } } }
                    }
                }
                article class="detail__pane" id="source" {
                    h2 { "Source" }
                    dl class="detail__metadata" {
                        dt { "Catalog source" } dd { (entry.provenance.source_url) }
                        dt { "Manifest digest" } dd code { (entry.provenance.manifest_digest) }
                        dt { "Activated at" } dd { (entry.provenance.activated_at) }
                        dt { "Entry count" } dd { (entry.provenance.entry_count) }
                        @if entry.provenance.embedded {
                            dt { "Source kind" } dd { "embedded recovery seed" }
                        }
                    }
                }
            }
        }
    }
}

fn format_bytes(value: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if value >= GIB {
        format!("{:.1} GiB", value as f64 / GIB as f64)
    } else if value >= MIB {
        format!("{:.1} MiB", value as f64 / MIB as f64)
    } else if value >= KIB {
        format!("{:.1} KiB", value as f64 / KIB as f64)
    } else {
        format!("{value} B")
    }
}

// ---------------------------------------------------------------------------
// Compatibility check route (used by the install wizard).
// ---------------------------------------------------------------------------

/// Pre-flight compatibility check used by the wizard. Returns the
/// compatibility report as JSON for the browser to render.
pub async fn compatibility(
    State(state): State<WebState>,
    WebUser(user, _): WebUser,
    Path(id): Path<String>,
    Query(params): Query<CompatibilityParams>,
) -> Response {
    let host = CompatibilityHost {
        platform: None,
        architecture: None,
        php_version: params.php_version.clone(),
        installed_packages: Vec::new(),
    };
    let report = state
        .software_center
        .compatibility(user.role(), &id, &params.version, host, Vec::new())
        .await
        .unwrap_or_default();
    json_response(report).into_response()
}

#[derive(Deserialize)]
pub struct CompatibilityParams {
    pub version: String,
    pub php_version: Option<String>,
}

fn json_response<T: serde::Serialize>(value: T) -> axum::Json<T> {
    axum::Json(value)
}

// ---------------------------------------------------------------------------
// Compatibility shim — preview/execute/cancel/retry/rollback routes that
// are still wired by the router for the existing kernel flows.
// ---------------------------------------------------------------------------

/// Render a digest-bound installation confirmation.
pub async fn preview(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<String>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let preview = match state
        .software_center
        .preview_install(user.id(), user.role(), &id)
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Review software transaction" }
        p { "Affected services: " (preview.affected_services.join(", ")) }
        code { (preview.plan.digest()) }
        form method="post" action={"/software/plans/" (preview.plan.digest()) "/execute"} {
            input type="hidden" name="_csrf" value=(csrf);
            input type="hidden" name="confirmation_token" value=(preview.confirmation_token);
            button { "Confirm installation" }
        }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// Render a digest-bound adoption, update, or removal confirmation.
pub async fn preview_component_action(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path((id, action)): Path<(String, openpanel_app::software_center::ComponentAction)>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let preview = match state
        .software_center
        .preview_component(user.id(), user.role(), &id, action)
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Review software transaction" }
        p { "Action: " (format!("{action:?}")) }
        p { "Affected services: " (preview.affected_services.join(", ")) }
        code { (preview.plan.digest()) }
        form method="post" action={"/software/plans/" (preview.plan.digest()) "/execute"} {
            input type="hidden" name="_csrf" value=(csrf);
            input type="hidden" name="confirmation_token" value=(preview.confirmation_token);
            button { "Confirm transaction" }
        }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// Browser application deployment choices.
#[derive(Deserialize)]
pub struct DeploymentPreviewForm {
    pub _csrf: String,
    pub application: String,
    pub domain: String,
    pub php_version: String,
    pub locale: String,
}

/// Render a complete application deployment plan for confirmation.
pub async fn preview_deployment(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<DeploymentPreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let preview = match state
        .software_center
        .preview_deployment(
            user.id(),
            user.role(),
            openpanel_app::software_center::ApplicationDeploymentInput {
                application: form.application,
                domain: form.domain,
                php_version: form.php_version,
                locale: form.locale,
                enable_dns: false,
                enable_tls: false,
                enable_backups: false,
            },
        )
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Review application deployment" }
        p { "Affected services: " (preview.affected_services.join(", ")) }
        code { (preview.plan.digest()) }
        form method="post" action={"/software/applications/plans/" (preview.plan.digest()) "/execute"} {
            input type="hidden" name="_csrf" value=(csrf);
            input type="hidden" name="confirmation_token" value=(preview.confirmation_token);
            button { "Confirm deployment" }
        }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// Execute one application deployment and render its one-time credentials.
pub async fn execute_deployment(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(digest): Path<String>,
    Form(form): Form<ExecuteForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let deployed = match state
        .software_center
        .execute_deployment(user.id(), user.role(), &digest, &form.confirmation_token)
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Application deployed" }
        p { "Save these administrator credentials now. They will not be shown again." }
        dl {
            dt { "Username" } dd { (deployed.admin_username) }
            dt { "Password" } dd { (deployed.admin_password) }
        }
        a href="/software" { "Return to Software Center" }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// CSRF-only form used to request a fresh preview.
#[derive(Deserialize)]
pub struct PreviewForm {
    pub _csrf: String,
}

/// Confirmed execution form.
#[derive(Deserialize)]
pub struct ExecuteForm {
    pub _csrf: String,
    pub confirmation_token: String,
}

/// Execute exactly the reviewed plan.
pub async fn execute(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(digest): Path<String>,
    Form(form): Form<ExecuteForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .software_center
        .execute(user.id(), user.role(), &digest, &form.confirmation_token)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    }
}

/// Request cancellation for an active job after CSRF validation.
pub async fn cancel(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<uuid::Uuid>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .software_center
        .cancel(user.id(), user.role(), id)
        .await
    {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

/// Generate and render a fresh confirmation for an interrupted job retry.
pub async fn retry(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<uuid::Uuid>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let preview = match state
        .software_center
        .retry_preview(user.id(), user.role(), id)
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::CONFLICT.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Review interrupted transaction retry" }
        p { "Packages: " (preview.packages.join(", ")) }
        code { (preview.plan_digest) }
        form method="post" action={"/software/plans/" (preview.plan_digest) "/execute"} {
            input type="hidden" name="_csrf" value=(csrf);
            input type="hidden" name="confirmation_token" value=(preview.confirmation_token);
            button { "Confirm retry" }
        }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// Run recipe-supported rollback for an interrupted component installation.
pub async fn rollback(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<uuid::Uuid>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .software_center
        .rollback_interrupted(user.id(), user.role(), id)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

// Re-export to keep the linter happy.
#[allow(dead_code)]
fn _arc_marker() {}
