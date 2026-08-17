//! Themeable UI web page: branding editor with palette preview.
//!
//! The page is a CSRF-protected form that posts to the JSON API
//! at `/api/v1/admin/branding`. The shell renders the user's
//! override by reading the override for the `Host` header via
//! `ThemeableUiService::resolve_for_host` (when a panel-domain
//! FQDN is bound) or via the caller's `UserId` (when there is
//! no FQDN).

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, header::HOST},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};

use crate::router::WebState;

/// Branding editor page.
pub async fn page(State(state): State<WebState>, headers: HeaderMap) -> Response {
    let host = headers
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(':').next().unwrap_or(s).to_string())
        .unwrap_or_default();
    // Try to resolve by FQDN first; fall back to the default
    // brand on the system host.
    let override_view: Markup = if host.is_empty() {
        override_view(None)
    } else {
        match state.themeable_ui.resolve_for_host(&host).await {
            Ok(Some(o)) => override_view(Some(o)),
            Ok(None) => override_view(None),
            Err(e) => html! {
                p class="form-error" role="alert" { "error: " (format!("{e:?}")) }
            },
        }
    };
    let body: Markup = html! {
        section class="card" {
            h2 { "Branding" }
            (override_view)
            form method="post" action="/api/v1/admin/branding" class="form form-grid" {
                label { "Brand name" input type="text" name="brand_name" required; }
                label { "Foreground (hex)"
                    input type="text" name="color_fg" placeholder="#RRGGBB" required;
                }
                label { "Background (hex)"
                    input type="text" name="color_bg" placeholder="#RRGGBB" required;
                }
                label { "Accent (hex)"
                    input type="text" name="color_accent" placeholder="#RRGGBB" required;
                }
                label { "Contrast minimum"
                    input type="number" name="contrast_min" min="1" max="21" step="0.1" value="4.5";
                }
                label { "Font family"
                    input type="text" name="font_family" value="system-ui" required;
                }
                label { "Base size (px)"
                    input type="number" name="base_size_px" min="10" max="24" value="16" required;
                }
                label { "Panel domain (optional FQDN)"
                    input type="text" name="panel_domain" placeholder="panel.acme.com";
                }
                div class="form-actions" {
                    button type="submit" { "Save override" }
                }
            }
        }
        section class="card" {
            h2 { "Logo upload" }
            form method="post" action="/api/v1/admin/branding/logo"
                  enctype="multipart/form-data" class="form" {
                label { "Logo file (SVG, PNG, or JPEG; \u{2264} 256 KiB)"
                    input type="file" name="file" accept="image/svg+xml,image/png,image/jpeg" required;
                }
                div class="form-actions" {
                    button type="submit" { "Upload" }
                }
            }
        }
    };
    (StatusCode::OK, body).into_response()
}

fn override_view(o: Option<openpanel_domain::ThemeOverride>) -> Markup {
    match o {
        Some(o) => html! {
            p { "Active override: "
                strong { (o.brand_name()) }
                " \u{2014} fg=" (o.palette().color_fg().as_hex())
                " bg=" (o.palette().color_bg().as_hex())
                " contrast=" (format!("{:.2}", o.palette().contrast_min()))
            }
            @if let Some(d) = o.panel_domain() {
                p { "Bound to FQDN: " code { (d.fqdn()) } }
            }
        },
        None => html! { p { "No override is stored for this host." } },
    }
}
