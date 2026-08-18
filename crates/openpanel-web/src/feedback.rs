//! Feedback widget: an NPS-style rating surface gated on account age and
//! per-browser dismissal, persisted via the `feedback` bounded context.

use axum::{
    Form,
    extract::State,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use openpanel_domain::{Sentiment, User, feedback::FEEDBACK_MIN_ACCOUNT_AGE_DAYS};
use serde::Deserialize;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// Success toast emitted with `HX-Trigger` after a persisted submission.
const FEEDBACK_TOAST_TRIGGER: &str = "layer-toast:{\"kind\":\"success\",\"msg\":\"Thanks for the feedback\",\"feedback\":\"submitted\"}";

/// Compute the account's age in days from its creation timestamp.
pub fn account_age_days(user: &User) -> i64 {
    (chrono::Utc::now() - user.created_at()).num_days()
}

/// Whether the server-side age gate lets this account see the widget.
pub fn age_gate_passes(age_days: i64) -> bool {
    age_days >= FEEDBACK_MIN_ACCOUNT_AGE_DAYS
}

/// The widget `<template>` the shell embeds in `#feedback-widget-root`.
/// The hydration script clones it into the container only when both the
/// age gate and the localStorage dismissal gate pass.
pub fn widget_template(csrf: &str) -> Markup {
    html! {
        template class="op-feedback-template" {
            (widget_form(csrf, None))
        }
    }
}

/// Render the widget form, optionally with an inline error.
fn widget_form(csrf: &str, error: Option<&str>) -> Markup {
    html! {
        form class="op-feedback-widget" id="feedback-widget" hx-post="/feedback"
            hx-target="#feedback-widget" hx-swap="outerHTML" {
            (csrf_field(csrf))
            p class="op-feedback-prompt" { "How is OpenPanel working for you?" }
            @if let Some(message) = error {
                p class="form-error" role="alert" { (message) }
            }
            fieldset class="op-feedback-ratings" {
                legend { "Rating" }
                label {
                    input type="radio" name="sentiment" value="up" required;
                    " Good"
                }
                label {
                    input type="radio" name="sentiment" value="down" required;
                    " Needs work"
                }
            }
            label {
                "Comment (optional)"
                textarea name="comment" rows="3" maxlength="2000";
            }
            div class="form-actions" {
                button type="submit" { "Send feedback" }
                button type="button" class="op-feedback-close" { "Dismiss" }
            }
        }
    }
}

/// Feedback submission form.
#[derive(Deserialize)]
pub struct FeedbackForm {
    _csrf: String,
    sentiment: Option<String>,
    comment: Option<String>,
}

/// POST /feedback — validate, rate-limit, and persist one submission.
pub async fn submit(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<FeedbackForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(sentiment) = form
        .sentiment
        .and_then(|value| Sentiment::parse(&value).ok())
    else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            widget_form(
                &state.csrf.token_for(session.id()),
                Some("Choose a rating."),
            )
            .into_string(),
        )
            .into_response();
    };
    match state
        .feedback
        .submit(user.id(), sentiment, form.comment)
        .await
    {
        Ok(_) => {
            let mut response = html! {
                div id="feedback-widget" class="op-feedback-widget" hidden role="status" {
                    "Thanks for the feedback!"
                }
            }
            .into_string()
            .into_response();
            response.headers_mut().insert(
                "HX-Trigger",
                HeaderValue::from_static(FEEDBACK_TOAST_TRIGGER),
            );
            response
        }
        Err(openpanel_domain::feedback::FeedbackError::RateLimited) => {
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                widget_form(
                    &state.csrf.token_for(session.id()),
                    Some("You've already shared feedback today. Thanks!"),
                )
                .into_string(),
            )
                .into_response();
            response.headers_mut().insert(
                "HX-Trigger",
                HeaderValue::from_static("form-validation-failed"),
            );
            response
        }
        Err(error) => (
            StatusCode::BAD_REQUEST,
            widget_form(
                &state.csrf.token_for(session.id()),
                Some(&format!("Could not save feedback: {error}")),
            )
            .into_string(),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_gate_uses_domain_threshold() {
        assert!(!age_gate_passes(2), "2 days below threshold");
        assert!(age_gate_passes(3), "3 days at threshold");
        assert!(age_gate_passes(30), "mature account passes");
        assert_eq!(FEEDBACK_MIN_ACCOUNT_AGE_DAYS, 3);
    }

    #[test]
    fn account_age_days_uses_creation_time() {
        let user = User::new(
            uuid::Uuid::new_v4(),
            openpanel_domain::Username::new("alice").expect("username"),
            openpanel_domain::Email::new("alice@example.com").expect("email"),
            openpanel_domain::Password::hash("correct horse battery staple").expect("password"),
            openpanel_domain::Role::User,
        );
        let days = account_age_days(&user);
        assert!(days >= 0, "new account is not negative: {days}");
    }

    #[test]
    fn widget_template_embeds_csrf_and_ratings() {
        let out = widget_template("tok123").into_string();
        assert!(out.contains("<template"), "template wrapper: {out}");
        assert!(
            out.contains("name=\"_csrf\" value=\"tok123\""),
            "csrf: {out}"
        );
        assert!(out.contains("name=\"sentiment\" value=\"up\""), "up: {out}");
        assert!(
            out.contains("name=\"sentiment\" value=\"down\""),
            "down: {out}"
        );
        assert!(out.contains("name=\"comment\""), "comment: {out}");
        assert!(out.contains("hx-post=\"/feedback\""), "post target: {out}");
        assert!(out.contains("op-feedback-close"), "dismiss button: {out}");
    }

    #[test]
    fn widget_form_renders_inline_error() {
        let out = widget_form("tok123", Some("Choose a rating.")).into_string();
        assert!(
            out.contains("class=\"form-error\" role=\"alert\""),
            "error: {out}"
        );
        assert!(out.contains("Choose a rating."), "message: {out}");
    }

    #[test]
    fn toast_trigger_is_parseable() {
        // The trigger must carry the JSON detail the layout listener expects.
        let detail = FEEDBACK_TOAST_TRIGGER
            .strip_prefix("layer-toast:")
            .expect("prefixed");
        let value: serde_json::Value = serde_json::from_str(detail).expect("valid json");
        assert_eq!(value["kind"], "success");
        assert_eq!(value["feedback"], "submitted");
    }
}
