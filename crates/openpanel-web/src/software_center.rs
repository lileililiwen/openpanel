//! Owner-only Software Center pages: storefront, detail, and install wizard.
//!
//! The page is rendered on the server; search, filter, sort, and refresh
//! all work as plain form GETs so the experience degrades gracefully
//! without JavaScript. HTMX is used for the install wizard and job
//! progress panel for a snappier feel when available.
#![allow(missing_docs)]

use std::collections::HashMap;

use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use openpanel_app::software_center::{
    CatalogQuery, CatalogSearchPage, CompatibilityHost, InstallBadge, StorefrontEntry,
};
use openpanel_domain::software_center::{CatalogHit, EntryKind};
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
    let require_verified_digests = state.software_center.require_verified_digests();
    let badges = match &page_result {
        Ok(page) => {
            let ids = page
                .hits
                .iter()
                .map(|hit| hit.id.as_str())
                .collect::<Vec<_>>();
            state
                .software_center
                .last_install_badges(user.role(), &ids)
                .await
                .unwrap_or_default()
        }
        Err(_) => HashMap::new(),
    };
    let content = match page_result {
        Ok(page) => storefront_content(
            &page,
            &params,
            diagnostics.as_ref(),
            &jobs,
            &csrf,
            require_verified_digests,
            &badges,
        ),
        Err(_) => error_content("Failed to load the Software Center catalog.", ""),
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
    require_verified_digests: bool,
    badges: &HashMap<String, InstallBadge>,
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
                        (card(hit, csrf, require_verified_digests, badges.get(&hit.id)))
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

fn card(
    hit: &CatalogHit,
    csrf: &str,
    require_verified_digests: bool,
    badge: Option<&InstallBadge>,
) -> Markup {
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
    let action = action_for_state(
        &state,
        hit.kind,
        &hit.id,
        csrf,
        require_verified_digests,
        hit.placeholder_digest,
    );
    html! {
        article class="card" {
            div class="card__icon" aria-hidden="true" { (category_glyph(hit.category.slug())) }
            header class="card__title" {
                h3 { a href={"/software/entries/" (hit.id)} { (hit.name) } }
                span class="card__meta" { (hit.latest_version) " · " (hit.license) }
            }
            p class="card__description" { (hit.description) }
            @if let Some(badge) = badge {
                div class="card__last-install" aria-label="Last install" {
                    "Last install: " (badge.display())
                }
            }
            div class="card__footer" {
                span class=(state_class) { (state_label) }
                (action)
            }
        }
    }
}

fn action_for_state(
    state: &str,
    kind: EntryKind,
    id: &str,
    csrf: &str,
    require_verified_digests: bool,
    placeholder_digest: bool,
) -> Markup {
    let install_blocked = require_verified_digests && placeholder_digest && kind == EntryKind::Web;
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
        "available" if kind == EntryKind::Web && install_blocked => html! {
            button class="button" disabled="disabled" title="recovery seed ships a placeholder digest; run software refresh against a remote catalog" { "Install (refresh required)" }
        },
        "available" if kind == EntryKind::Web => html! {
            form method="post" action={"/software/components/" (id) "/install"} {
                input type="hidden" name="_csrf" value=(csrf);
                button class="button" { "Install" }
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

fn error_content(message: &str, _csrf: &str) -> Markup {
    html! {
        div class="error-state" {
            h1 { "Software Center unavailable" }
            p { (message) }
            a class="button" href="/software" { "Return to Software Center" }
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
    let require_verified_digests = state.software_center.require_verified_digests();
    let badge = state
        .software_center
        .last_install_badge(user.role(), &id)
        .await
        .ok()
        .flatten();
    let content = detail_content(&entry, &csrf, require_verified_digests, badge.as_ref());
    state
        .render_shell(&user, &csrf, &format!("/software/entries/{id}"), content)
        .await
        .into_response()
}

/// Render the application deployment form for a Web entry. The form
/// posts to `/software/applications/preview` which assembles the plan and
/// returns the digest-bound confirmation token used by the existing
/// deployment executor.
pub async fn deploy_form(
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
    let content = deploy_form_content(&entry, &csrf);
    let path = format!("/software/components/{id}/deploy");
    state
        .render_shell(&user, &csrf, &path, content)
        .await
        .into_response()
}

fn deploy_form_content(entry: &StorefrontEntry, csrf: &str) -> Markup {
    html! {
        article class="storefront__detail" {
            header class="detail__header" {
                h1 { "Deploy " (entry.name) }
                span class="badge" { (entry.category.label()) }
                span class="badge" { (entry.kind.as_str()) }
                span class="badge" { (entry.license) }
            }
            p class="detail__lead" { (entry.description) }
            p { "Provide the destination domain and runtime options. The next page will summarize the full transaction before any change is made." }
            form method="post" action="/software/applications/preview" class="deploy-form" {
                input type="hidden" name="_csrf" value=(csrf);
                input type="hidden" name="application" value=(entry.id);
                label class="field" {
                    span { "Domain" }
                    input type="text" name="domain" required="required"
                      pattern="[a-z0-9.-]+" placeholder="example.com";
                }
                label class="field" {
                    span { "PHP version" }
                    input type="text" name="php_version" required="required"
                      pattern=r"8\.[34]" placeholder="8.3" value="8.3";
                }
                label class="field" {
                    span { "Locale" }
                    input type="text" name="locale" required="required"
                      pattern="[A-Za-z0-9_-]+" placeholder="en_US" value="en_US";
                }
                div class="deploy-form__actions" {
                    a class="button button--ghost" href={"/software/entries/" (entry.id)} { "Cancel" }
                    button class="button" { "Review deployment" }
                }
            }
        }
    }
}

/// One-click download-and-place for any Web entry that ships an
/// `ArtifactPin`. The handler does the whole flow in one round trip:
/// fetch the pinned URL, verify the digest (or skip the check when the
/// recipe ships a documented placeholder), and drop the file or
/// extracted archive at the managed webapps root. No preview, no
/// confirmation token, no wizard.
pub async fn install_artifact(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<String>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let result = state
        .software_center
        .install_artifact(user.id(), user.role(), &id)
        .await;
    let content = match result {
        Ok(installed) => html! {
            h1 { "Software installed" }
            p { (installed.entry_name) " " (installed.version) " was downloaded and placed on this host." }
            dl class="detail__metadata" {
                dt { "Entry" } dd { (installed.entry_id) }
                dt { "Version" } dd { (installed.version) }
                dt { "Archive" } dd { (installed.archive_type) }
                dt { "Bytes" } dd { (installed.bytes) }
                dt { "Digest verified" } dd { @if installed.digest_verified { "yes" } @else { "skipped (placeholder)" } }
                dt { "Path" } dd code { (installed.destination.display()) }
                @if let Some(name) = &installed.filename {
                    dt { "Filename" } dd code { (name) }
                }
            }
            div class="detail__action" {
                a class="button" href="/software" { "Return to Software Center" }
            }
        },
        Err(error) => error_content(&software_center_error_message(&error), &csrf),
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

fn detail_content(
    entry: &StorefrontEntry,
    csrf: &str,
    require_verified_digests: bool,
    badge: Option<&InstallBadge>,
) -> Markup {
    let install_blocked = require_verified_digests
        && entry.versions.iter().any(|version| {
            version
                .artifact
                .as_ref()
                .map(|pin| pin.sha256 == openpanel_app::software_center::PLACEHOLDER_SHA256)
                .unwrap_or(false)
        })
        && entry.kind == EntryKind::Web;
    let install_action = match entry.install_state.as_str() {
        "available" if entry.kind == EntryKind::Web && install_blocked => Some(html! {
            div class="detail__notice" {
                p { "This entry is not installed because the recovery seed does not pin a real SHA-256. Run \u{201c}software refresh\u{201d} against a remote catalog that pins a digest, or set OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS=false to opt back into the lenient behavior for air-gapped recovery." }
            }
            button class="button" disabled="disabled" { "Install (refresh required)" }
        }),
        "available" if entry.kind == EntryKind::Web => Some(html! {
            form method="post" action={"/software/components/" (entry.id) "/install"} {
                input type="hidden" name="_csrf" value=(csrf);
                button class="button" { "Install" }
            }
        }),
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
            @if let Some(badge) = badge {
                div class="detail__last-install" aria-label="Last install" {
                    "Last install: " (badge.display())
                }
            }
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
            button class="button" { "Confirm installation" }
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
            button class="button" { "Confirm transaction" }
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
            button class="button" { "Confirm deployment" }
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
    let csrf = state.csrf.token_for(session.id());
    let result = state
        .software_center
        .execute_deployment(user.id(), user.role(), &digest, &form.confirmation_token)
        .await;
    let content = match result {
        Ok(deployed) => html! {
            h1 { "Application deployed" }
            p { "Save these administrator credentials now. They will not be shown again." }
            dl {
                dt { "Username" } dd { (deployed.admin_username) }
                dt { "Password" } dd { (deployed.admin_password) }
            }
            a class="button" href="/software" { "Return to Software Center" }
        },
        Err(error) => error_content(&software_center_error_message(&error), &csrf),
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
    let csrf = state.csrf.token_for(session.id());
    let result = state
        .software_center
        .execute(user.id(), user.role(), &digest, &form.confirmation_token)
        .await;
    let content = match result {
        Ok(job) => html! {
            h1 { "Installation complete" }
            p { "The transaction finished and the host is back in a healthy state." }
            dl class="detail__metadata" {
                dt { "Job" } dd code { (job.id) }
                dt { "State" } dd { (job.state) }
                dt { "Plan digest" } dd code { (job.plan_digest) }
            }
            div class="detail__action" {
                a class="button" href="/software" { "Return to Software Center" }
            }
        },
        Err(error) => error_content(&software_center_error_message(&error), &csrf),
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// User-friendly translation of a [`SoftwareCenterError`] so the operator
/// can read the real failure on the confirmation page instead of staring
/// at a blank 422. The error is rendered through the authed shell so the
/// user never has to leave the Software Center to recover.
fn software_center_error_message(
    error: &openpanel_app::software_center::SoftwareCenterError,
) -> String {
    use openpanel_app::software_center::SoftwareCenterError;
    match error {
        SoftwareCenterError::Forbidden => {
            "Only an Owner may execute this transaction.".to_owned()
        }
        SoftwareCenterError::Invalid(detail) => {
            if detail.is_empty() {
                "The confirmation token is no longer valid. Open the entry again and confirm the freshly generated plan.".to_owned()
            } else {
                detail.clone()
            }
        }
        SoftwareCenterError::Conflict => {
            "The host changed since the preview was generated. Open the entry to request a fresh plan before retrying.".to_owned()
        }
        SoftwareCenterError::Dependencies(count) => format!(
            "{count} managed component{} still depend on this one. Remove or migrate them first.",
            if *count == 1 { "" } else { "s" }
        ),
        SoftwareCenterError::Validation => {
            "The host rejected the installed package. Inspect the audit log and retry the plan.".to_owned()
        }
        SoftwareCenterError::Package(detail) => {
            if detail.is_empty() {
                "The package adapter refused the transaction. The host was rolled back to the previous state.".to_owned()
            } else {
                format!("The package adapter refused the transaction. The host was rolled back to the previous state. Detail: {detail}")
            }
        }
        SoftwareCenterError::Unsupported => {
            "This entry's deployment adapter is not configured on this host.".to_owned()
        }
        SoftwareCenterError::Repository => {
            "The Software Center could not persist the job state. Try again; if it persists, the audit log has the trace.".to_owned()
        }
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
            button class="button" { "Confirm retry" }
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
