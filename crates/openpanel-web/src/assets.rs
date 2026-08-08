//! Static assets served to the browser (HTMX + the panel stylesheet).

use axum::{
    body::Bytes,
    http::{HeaderValue, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};

const HTMX_MIN_JS: &[u8] = include_bytes!("../assets/htmx.min.js");
const APP_CSS: &[u8] = include_bytes!("../assets/app.css");

/// GET /assets/htmx.min.js
pub async fn htmx_min_js() -> Response {
    (
        [(
            CONTENT_TYPE,
            HeaderValue::from_static("application/javascript"),
        )],
        Bytes::from_static(HTMX_MIN_JS),
    )
        .into_response()
}

/// GET /assets/app.css
pub async fn app_css() -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("text/css"))],
        Bytes::from_static(APP_CSS),
    )
        .into_response()
}
