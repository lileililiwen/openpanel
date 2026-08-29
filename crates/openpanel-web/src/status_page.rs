//! Public, unauthenticated status page rendered at `/status/{slug}`.
//!
//! The slug is a 128-bit random base32 token that the operator can
//! rotate. Anonymous visitors see the operator-chosen labels and the
//! current status / 90-day uptime bars / active incidents. Internal
//! check ids and target URLs are never rendered.
//!
//! Returns 404 for both disabled pages and unknown slugs — the body
//! is identical so the slug cannot be enumerated.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use maud::{DOCTYPE, Markup, html};
use openpanel_app::{StatusPageService, synthetic_monitoring::PublicStatusView};
use openpanel_domain::synthetic_monitoring::DailyBar;

/// Render the public status page with `Arc<StatusPageService>` as state.
/// This is the entry point for the unauthenticated public router.
pub async fn page_with_status_page(
    State(svc): State<Arc<StatusPageService>>,
    Path(slug): Path<String>,
) -> Response {
    match svc.public_view_for(&slug).await {
        Ok(view) => render_success(view).await,
        Err(_) => render_not_found(),
    }
}

async fn render_success(view: PublicStatusView) -> Response {
    let content = html! {
        main class="status-public" {
            h1 { "Service status" }
            @if view.entries.is_empty() {
                p class="status-empty" {
                    "No checks published yet."
                }
            } @else {
                @for entry in &view.entries {
                    section class="status-entry" {
                        h2 class="status-label" { (entry.label) }
                        p class={ "status-current status-" (status_class(&entry.current_status)) } {
                            (status_text(&entry.current_status))
                        }
                        @if let Some(last) = entry.last_ran_at {
                            p class="status-last-ran" {
                                "Last check: " (format_timestamp(last))
                            }
                        }
                        div class="status-bars" {
                            @for bar in &entry.uptime_bars {
                                @let class = match bar.uptime {
                                    Some(uptime) if uptime >= 0.999 => "bar bar-ok",
                                    Some(_) => "bar bar-warn",
                                    None => "bar bar-no-data",
                                };
                                div class=(class) title={ (format_bar_title(bar)) } { }
                            }
                        }
                    }
                }
            }
            footer class="status-footer" {
                p { "Generated at " (format_timestamp(view.generated_at)) }
            }
        }
    };
    let body = bare_shell(content);
    let mut response = body.into_response();
    if let Ok(value) = HeaderValue::from_str("public, max-age=30") {
        response.headers_mut().insert(header::CACHE_CONTROL, value);
    }
    response
}

fn render_not_found() -> Response {
    let content = html! {
        main class="status-public" {
            h1 { "Service status" }
            p { "Page not found." }
        }
    };
    let body = bare_shell(content);
    (
        StatusCode::NOT_FOUND,
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=30"),
        )],
        body.into_string(),
    )
        .into_response()
}

fn bare_shell(content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Service status" }
            }
            body {
                (content)
            }
        }
    }
}

fn status_class(status: &openpanel_domain::CheckStatus) -> &'static str {
    match status {
        openpanel_domain::CheckStatus::Ok => "ok",
        openpanel_domain::CheckStatus::Warn => "warn",
        openpanel_domain::CheckStatus::Fail => "fail",
    }
}

fn status_text(status: &openpanel_domain::CheckStatus) -> &'static str {
    match status {
        openpanel_domain::CheckStatus::Ok => "Operational",
        openpanel_domain::CheckStatus::Warn => "Degraded",
        openpanel_domain::CheckStatus::Fail => "Down",
    }
}

fn format_timestamp(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

fn format_bar_title(bar: &DailyBar) -> String {
    match bar.uptime {
        Some(uptime) => format!("{}: {:.1}%", bar.day, uptime * 100.0),
        None => format!("{}: no data", bar.day),
    }
}
