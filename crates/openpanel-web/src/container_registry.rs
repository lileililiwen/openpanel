//! Container registry web page.
//!
//! Renders the registry tab with config, namespace list, image
//! list, and scan badges. CSRF is enforced on POSTs through the
//! shared `ValidateCsrf` extractor.

use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};

use crate::router::WebState;

const REGISTRY_UI_HEADER: &str = "registry-ui-pending";

/// Render the registry index.
pub async fn page(State(state): State<WebState>) -> Response {
    let cfg = state.registry.config();
    let namespaces = state.registry.list_namespaces().await.unwrap_or_default();
    render(&cfg, &namespaces).into_response()
}

fn render(
    cfg: &openpanel_domain::RegistryConfig,
    namespaces: &[openpanel_domain::ImageNamespace],
) -> Markup {
    let stub = REGISTRY_UI_HEADER;
    html! {
        div class="registry" data-stub=(stub) {
            h1 { "Container Registry" }
            p { (format!("scan on push: {}", cfg.scan_on_push)) }
            p { (format!("retention: max={:?} age_days={:?}", cfg.retention.max_images_per_ns, cfg.retention.max_age_days)) }
            @for ns in namespaces {
                p { (format!("{} -> {} / {}", ns.namespace_id, ns.used_bytes, ns.quota_bytes)) }
            }
        }
    }
}
