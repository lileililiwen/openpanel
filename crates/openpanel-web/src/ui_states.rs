//! Reusable UI-state vocabulary: empty, no-results, loading, and error
//! components. Every list route renders one of these instead of a blank
//! table or blank panel.
//!
//! Stable CSS classes and ARIA live regions are guaranteed here so
//! assistive technology announces transitions and the styling contract
//! (`op-empty-state`, `op-no-results`, `op-loading-state`,
//! `op-error-state`) has a single implementation.

use maud::{Markup, html};

/// Empty list state with an optional call-to-action.
#[derive(Debug, Clone, Copy)]
pub struct EmptyState<'a> {
    /// Heading shown in the empty panel.
    pub title: &'a str,
    /// Supporting explanation.
    pub body: &'a str,
    /// Destination of the primary call-to-action.
    pub cta_href: &'a str,
    /// Label of the primary call-to-action.
    pub cta_label: &'a str,
}

impl<'a> EmptyState<'a> {
    /// Build an empty state with a heading and body.
    pub fn new(title: &'a str, body: &'a str) -> Self {
        Self {
            title,
            body,
            cta_href: "",
            cta_label: "",
        }
    }

    /// Attach a call-to-action link.
    pub fn with_cta(mut self, href: &'a str, label: &'a str) -> Self {
        self.cta_href = href;
        self.cta_label = label;
        self
    }

    /// Render the empty-state panel.
    pub fn render(&self) -> Markup {
        html! {
            section class="op-empty-state" aria-live="polite" {
                h2 { (self.title) }
                p { (self.body) }
                @if !self.cta_href.is_empty() {
                    a class="btn" href=(self.cta_href) { (self.cta_label) }
                }
            }
        }
    }
}

/// Filtered list with zero matches, offering a way back to the full list.
#[derive(Debug, Clone, Copy)]
pub struct NoResultsState<'a> {
    /// The query that returned no rows.
    pub query: &'a str,
    /// Base list href whose filter is cleared by this link.
    pub clear_href: &'a str,
}

impl<'a> NoResultsState<'a> {
    /// Build a no-results state for a query.
    pub fn new(query: &'a str) -> Self {
        Self {
            query,
            clear_href: "",
        }
    }

    /// Attach the clear-filter destination.
    pub fn with_clear_href(mut self, href: &'a str) -> Self {
        self.clear_href = href;
        self
    }

    /// Render the no-results panel.
    pub fn render(&self) -> Markup {
        html! {
            section class="op-no-results" aria-live="polite" {
                h2 { "No results" }
                p { "No entries matched your query " code { (self.query) } "." }
                @if !self.clear_href.is_empty() {
                    a class="btn" href=(self.clear_href) { "Clear filter" }
                }
            }
        }
    }
}

/// Visible loading indicator used by `hx-indicator` during requests.
#[derive(Debug, Clone, Copy)]
pub struct LoadingState<'a> {
    /// Accessible label describing what is loading.
    pub label: &'a str,
}

impl<'a> LoadingState<'a> {
    /// Build a loading state with an accessible label.
    pub fn new(label: &'a str) -> Self {
        Self { label }
    }

    /// Render the loading indicator. The `htmx-request` class is toggled
    /// by HTMX while a request targeting this indicator is in flight.
    pub fn render(&self) -> Markup {
        html! {
            div class="op-loading-state" aria-live="polite" role="status" {
                span class="op-loading-spinner" aria-hidden="true";
                (self.label)
            }
        }
    }
}

/// Recoverable error state with a retry action.
#[derive(Debug, Clone, Copy)]
pub struct ErrorState<'a> {
    /// Short error heading.
    pub title: &'a str,
    /// Safe, user-facing explanation (never a raw stack trace).
    pub body: &'a str,
    /// Same endpoint to retry.
    pub retry_href: &'a str,
}

impl<'a> ErrorState<'a> {
    /// Build an error state with a title, body, and retry destination.
    pub fn new(title: &'a str, body: &'a str, retry_href: &'a str) -> Self {
        Self {
            title,
            body,
            retry_href,
        }
    }

    /// Render the error state.
    pub fn render(&self) -> Markup {
        html! {
            section class="op-error-state" aria-live="assertive" role="alert" {
                h2 { (self.title) }
                p { (self.body) }
                a class="btn" hx-get=(self.retry_href) hx-target="#content" { "Retry" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_renders_stable_class_and_aria() {
        let out = EmptyState::new("No sites yet", "Create your first site")
            .with_cta("/sites/new", "Create your first site")
            .render()
            .into_string();
        assert!(
            out.contains("class=\"op-empty-state\""),
            "stable class: {out}"
        );
        assert!(out.contains("aria-live=\"polite\""), "polite live: {out}");
        assert!(out.contains("No sites yet"), "title: {out}");
        assert!(out.contains("href=\"/sites/new\""), "cta: {out}");
    }

    #[test]
    fn empty_state_without_cta_renders_no_link() {
        let out = EmptyState::new("No backups", "Create a plan")
            .render()
            .into_string();
        assert!(out.contains("op-empty-state"), "class: {out}");
        assert!(!out.contains("<a"), "no cta link: {out}");
    }

    #[test]
    fn no_results_renders_query_and_clear_link() {
        let out = NoResultsState::new("zzz")
            .with_clear_href("/sites")
            .render()
            .into_string();
        assert!(out.contains("class=\"op-no-results\""), "class: {out}");
        assert!(out.contains("aria-live=\"polite\""), "live: {out}");
        assert!(out.contains("zzz"), "query echoed: {out}");
        assert!(out.contains("href=\"/sites\""), "clear link: {out}");
        assert!(out.contains("Clear filter"), "label: {out}");
    }

    #[test]
    fn loading_state_renders_status_and_label() {
        let out = LoadingState::new("Creating site…").render().into_string();
        assert!(out.contains("class=\"op-loading-state\""), "class: {out}");
        assert!(out.contains("aria-live=\"polite\""), "live: {out}");
        assert!(out.contains("role=\"status\""), "status role: {out}");
        assert!(out.contains("Creating site…"), "label: {out}");
        assert!(out.contains("op-loading-spinner"), "spinner: {out}");
    }

    #[test]
    fn error_state_uses_assertive_aria_and_retry() {
        let out = ErrorState::new("Something went wrong", "Please retry", "/sites")
            .render()
            .into_string();
        assert!(out.contains("class=\"op-error-state\""), "class: {out}");
        assert!(out.contains("aria-live=\"assertive\""), "assertive: {out}");
        assert!(out.contains("role=\"alert\""), "alert role: {out}");
        assert!(out.contains("hx-get=\"/sites\""), "retry wired: {out}");
        assert!(out.contains("Retry"), "retry label: {out}");
    }

    #[test]
    fn all_four_states_share_no_css_leakage() {
        // Each state owns its own class; no state's markup leaks another's.
        let empty = EmptyState::new("t", "b").render().into_string();
        let no_results = NoResultsState::new("q").render().into_string();
        let loading = LoadingState::new("l").render().into_string();
        let error = ErrorState::new("t", "b", "/r").render().into_string();
        assert!(!empty.contains("op-no-results") && !empty.contains("op-loading-state"));
        assert!(!no_results.contains("op-empty-state") && !no_results.contains("op-error-state"));
        assert!(!loading.contains("op-error-state") && !loading.contains("op-no-results"));
        assert!(!error.contains("op-empty-state") && !error.contains("op-loading-state"));
    }
}
