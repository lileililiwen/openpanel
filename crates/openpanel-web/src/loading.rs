//! Loading-indicator helper: a named `hx-indicator` element paired with
//! [`crate::ui_states::LoadingState`]. Forms that may take longer than
//! 250 ms include `hx-indicator="#op-loading-{action}"` and mount the
//! matching indicator via [`crate::loading::loading_indicator`].

use maud::{Markup, html};

use crate::ui_states::LoadingState;

/// Render a hidden loading indicator for an HTMX action.
///
/// HTMX adds the `htmx-request` class to this element while the request
/// is in flight; `app.css` only displays `.op-loading-state` while that
/// class is present, so the indicator is invisible otherwise.
pub fn loading_indicator(action: &str, label: &str) -> Markup {
    let id = format!("op-loading-{action}");
    html! {
        div id=(id) class="op-loading-state" hidden {
            (LoadingState::new(label).render())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicator_renders_named_hidden_state() {
        let out = loading_indicator("create-site", "Creating site…").into_string();
        assert!(out.contains("id=\"op-loading-create-site\""), "id: {out}");
        assert!(
            out.contains("class=\"op-loading-state\""),
            "state class: {out}"
        );
        assert!(out.contains("hidden"), "hidden by default: {out}");
        assert!(out.contains("Creating site…"), "label: {out}");
    }
}
