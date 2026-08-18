//! Inline form validation: reusable per-field error/ok fragments and the
//! `422` server validation contract.
//!
//! Every form route renders its field errors through [`render_field_error`]
//! (or clears them through [`render_field_ok`]) so the markup is
//! structurally identical across routes. Blur-triggered validation hits
//! `POST /forms/validate`; full form handlers return
//! [`validation_error_response`] on validation failure.

use axum::{
    Form,
    extract::State,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use serde::Deserialize;
use serde_json::Value;

use crate::router::{WebState, WebUser};

/// Render the inline error element for one field. Structurally stable
/// across every field and message: same class, same id scheme, same ARIA.
pub fn render_field_error(field: &str, msg: &str) -> Markup {
    html! {
        div class="field-error" id=(format!("field-error-{field}")) role="alert" { (msg) }
    }
}

/// Render the empty inline marker for one field, used to clear a
/// previously rendered error via the same swap target.
pub fn render_field_ok(field: &str) -> Markup {
    html! {
        div class="field-error" id=(format!("field-error-{field}")) aria-hidden="true";
    }
}

/// Build the canonical `422 Unprocessable Entity` response for a form
/// handler: JSON body `{ "errors": { "<field>": "<message>", ... } }`
/// plus the `HX-Trigger: form-validation-failed` header so client-side
/// handlers can attach focus management.
pub fn validation_error_response(errors: &[(String, String)]) -> Response {
    let map = errors
        .iter()
        .map(|(field, message)| (field.clone(), Value::String(message.clone())))
        .collect::<serde_json::Map<String, Value>>();
    let body = serde_json::json!({ "errors": map });
    let mut response = (StatusCode::UNPROCESSABLE_ENTITY, body.to_string()).into_response();
    response.headers_mut().insert(
        "HX-Trigger",
        HeaderValue::from_static("form-validation-failed"),
    );
    response
}

/// Wire value kind for blur validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    /// Any non-empty value.
    Required,
    /// Basic email shape.
    Email,
    /// Hostname shape.
    Domain,
    /// At least 8 characters.
    Password,
}

impl FieldKind {
    fn parse(value: &str) -> Self {
        match value {
            "email" => Self::Email,
            "domain" => Self::Domain,
            "password" => Self::Password,
            _ => Self::Required,
        }
    }

    /// Validate a value, returning the error message when invalid.
    fn error_message(self, value: &str) -> Option<&'static str> {
        match self {
            Self::Required if value.trim().is_empty() => Some("This field is required"),
            Self::Email if !is_email(value.trim()) => Some("Enter a valid email address"),
            Self::Domain if !is_domain(value.trim()) => Some("Enter a valid hostname"),
            Self::Password if value.len() < 8 => Some("Password must be at least 8 characters"),
            _ => None,
        }
    }
}

fn is_email(value: &str) -> bool {
    value.len() <= 254
        && !value.chars().any(char::is_control)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && is_domain(domain) && !domain.contains(':')
        })
}

fn is_domain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && !value.contains("--")
}

/// Single-field blur validation form.
#[derive(Deserialize)]
pub struct ValidateFieldForm {
    /// Field name, echoed into the error element id.
    field: String,
    /// Current field value.
    value: String,
    /// Validation kind (`required`, `email`, `domain`, `password`).
    kind: Option<String>,
}

/// POST /forms/validate — validate one field on blur and return the OOB
/// fragment carrying the inline error (or the empty ok marker).
pub async fn validate(
    State(_state): State<WebState>,
    WebUser(_user, _session): WebUser,
    Form(form): Form<ValidateFieldForm>,
) -> Response {
    let kind = FieldKind::parse(form.kind.as_deref().unwrap_or("required"));
    match kind.error_message(&form.value) {
        Some(message) => {
            let mut response = (
                StatusCode::UNPROCESSABLE_ENTITY,
                render_field_error(&form.field, message).into_string(),
            )
                .into_response();
            response.headers_mut().insert(
                "HX-Trigger",
                HeaderValue::from_static("form-validation-failed"),
            );
            let errors = serde_json::json!({ "errors": { form.field.as_str(): message } });
            if let Ok(value) = HeaderValue::from_str(&errors.to_string()) {
                response.headers_mut().insert("X-Form-Errors", value);
            }
            response
        }
        None => (StatusCode::OK, render_field_ok(&form.field).into_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_field_error_is_structural_across_fixtures() {
        let a = render_field_error("email", "not an email").into_string();
        let b = render_field_error("domain", "bad hostname").into_string();
        for out in [&a, &b] {
            assert!(out.contains("class=\"field-error\""), "class: {out}");
            assert!(out.contains("role=\"alert\""), "alert role: {out}");
        }
        assert!(a.contains("id=\"field-error-email\""), "email id: {a}");
        assert!(b.contains("id=\"field-error-domain\""), "domain id: {b}");
    }

    #[test]
    fn render_field_ok_is_structural_across_fixtures() {
        let a = render_field_ok("email").into_string();
        let b = render_field_ok("domain").into_string();
        for out in [&a, &b] {
            assert!(out.contains("class=\"field-error\""), "class: {out}");
            assert!(out.contains("aria-hidden=\"true\""), "hidden: {out}");
        }
        assert!(a.contains("id=\"field-error-email\""), "email id: {a}");
        assert!(b.contains("id=\"field-error-domain\""), "domain id: {b}");
    }

    #[test]
    fn validation_error_response_is_422_json_with_trigger() {
        let errors = vec![
            ("domain".to_string(), "invalid domain".to_string()),
            ("document_root".to_string(), "required".to_string()),
        ];
        let response = validation_error_response(&errors);
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let header = response
            .headers()
            .get("HX-Trigger")
            .expect("trigger header")
            .to_str()
            .expect("ascii")
            .to_string();
        assert_eq!(header, "form-validation-failed");
    }

    #[test]
    fn field_kinds_validate_values() {
        assert_eq!(
            FieldKind::Required.error_message("  "),
            Some("This field is required")
        );
        assert_eq!(FieldKind::Required.error_message("x"), None);
        assert_eq!(
            FieldKind::Email.error_message("nope"),
            Some("Enter a valid email address")
        );
        assert_eq!(FieldKind::Email.error_message("a@b.co"), None);
        assert_eq!(
            FieldKind::Domain.error_message("bad..host"),
            Some("Enter a valid hostname")
        );
        assert_eq!(FieldKind::Domain.error_message("example.com"), None);
        assert_eq!(
            FieldKind::Password.error_message("short"),
            Some("Password must be at least 8 characters")
        );
        assert_eq!(FieldKind::Password.error_message("long-enough-pw"), None);
    }
}
