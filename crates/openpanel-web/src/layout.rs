//! Shared HTML chrome: the HTMX shell (sidebar + topbar + content region)
//! plus the `csrf_field` helper used by every state-changing form.
//!
//! The navigation model (sections, items, icons) lives in
//! [`crate::nav_model`]; this module renders it.

use std::collections::BTreeSet;

use maud::{DOCTYPE, Markup, html};
use openpanel_domain::Role;

use crate::nav_model::{NAV_SECTIONS, NavItem, NavSection, icon_svg};

/// Registered browser capabilities used to suppress unavailable navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilitySet(BTreeSet<&'static str>);

impl CapabilitySet {
    /// Capabilities currently shipped by the OpenPanel composition root.
    pub fn shipped() -> Self {
        Self(BTreeSet::from([
            "dashboard",
            "sites",
            "files",
            "databases",
            "ssl",
            "monitoring",
            "users",
            "settings",
        ]))
    }

    /// Add a registered capability, used as new modules join the web router.
    pub fn with(mut self, capability: &'static str) -> Self {
        self.0.insert(capability);
        self
    }

    fn contains(&self, capability: &str) -> bool {
        self.0.contains(capability)
    }
}

/// Vanilla JS that powers the sidebar filter, section-collapse
/// persistence, and the compact rail toggle. Deliberately tiny and
/// framework-free; it only manipulates classes/attributes and
/// localStorage, never page content.
const NAV_JS: &str = r#"
(function () {
  var filter = document.getElementById('nav-filter');
  if (filter) {
    filter.addEventListener('input', function () {
      var q = filter.value.trim().toLowerCase();
      document.querySelectorAll('.nav-item').forEach(function (a) {
        var label = (a.getAttribute('data-label') || '').toLowerCase();
        a.hidden = q !== '' && label.indexOf(q) === -1;
      });
      document.querySelectorAll('.nav-section').forEach(function (s) {
        if (q === '') { s.hidden = false; return; }
        s.hidden = s.querySelectorAll('.nav-item:not([hidden])').length === 0;
      });
    });
  }
  document.querySelectorAll('.nav-section').forEach(function (s) {
    var key = 'openpanel.nav.' + (s.getAttribute('data-section') || '') + '.open';
    try {
      var saved = localStorage.getItem(key);
      if (saved === '0') { s.removeAttribute('open'); }
      else if (saved === '1') { s.setAttribute('open', ''); }
    } catch (e) {}
    s.addEventListener('toggle', function () {
      try { localStorage.setItem(key, s.open ? '1' : '0'); } catch (e) {}
    });
  });
  var rail = document.getElementById('nav-rail-toggle');
  if (rail) {
    var rk = 'openpanel.nav.rail';
    try {
      if (localStorage.getItem(rk) === '1') {
        document.body.classList.add('nav-rail');
        rail.setAttribute('aria-pressed', 'true');
      }
    } catch (e) {}
    rail.addEventListener('click', function () {
      var on = document.body.classList.toggle('nav-rail');
      rail.setAttribute('aria-pressed', on ? 'true' : 'false');
      try { localStorage.setItem(rk, on ? '1' : '0'); } catch (e) {}
    });
  }
})();
"#;

/// Render a hidden `_csrf` input carrying the current session's token.
/// Every state-changing web form MUST include this field.
pub fn csrf_field(token: &str) -> Markup {
    html! {
        input type="hidden" name="_csrf" value=(token);
    }
}

/// Inline JS that wires the shell's interaction surface: the
/// `htmx:afterRequest` toast hook, confirm-modal teardown, and the
/// feedback-widget hydration gate. Namespaced to a single listener so it
/// never duplicates other handlers.
const LAYER_JS: &str = r#"
(function () {
  document.body.addEventListener('htmx:afterRequest', function (evt) {
    var xhr = evt.detail && evt.detail.xhr;
    if (!xhr) return;
    var trigger = xhr.getResponseHeader('HX-Trigger');
    if (!trigger || trigger.indexOf('layer-toast') === -1) return;
    var detail = {};
    var idx = trigger.indexOf(':');
    if (idx !== -1) {
      try { detail = JSON.parse(trigger.slice(idx + 1)); } catch (e) {}
    }
    if (detail.feedback === 'submitted') {
      try { localStorage.setItem('openpanel_feedback_seen', 'true'); } catch (e) {}
    }
    var kind = detail.kind || 'success';
    var msg = detail.msg || 'Done';
    fetch('/layer/toast?kind=' + encodeURIComponent(kind) + '&msg=' + encodeURIComponent(msg))
      .then(function (r) { return r.text(); })
      .then(function (html) {
        var root = document.getElementById('layer-root');
        if (!root) return;
        root.insertAdjacentHTML('beforeend', html);
        root.querySelectorAll('[data-op-auto-dismiss]').forEach(function (t) {
          setTimeout(function () { t.remove(); }, 4000);
          t.addEventListener('click', function () { t.remove(); });
        });
      })
      .catch(function (e) { console.error('layer-toast fetch failed', e); });
  });

  document.body.addEventListener('htmx:afterRequest', function (evt) {
    var root = document.getElementById('layer-root');
    var elt = evt.detail && evt.detail.elt;
    if (!root || !elt || !evt.detail.successful) return;
    var inModal = (elt.closest && elt.closest('.op-modal')) ||
      (elt.hasAttribute && elt.hasAttribute('data-op-confirm-form'));
    if (inModal) { root.innerHTML = ''; }
  });

  document.addEventListener('click', function (e) {
    var target = e.target;
    var close = target.closest && target.closest('.op-feedback-close');
    if (!close) return;
    try { localStorage.setItem('openpanel_feedback_seen', 'true'); } catch (e) {}
    var widget = document.getElementById('feedback-widget');
    if (widget) { widget.remove(); }
    var root = document.getElementById('feedback-widget-root');
    if (root) { root.innerHTML = ''; }
  });

  var feedbackRoot = document.getElementById('feedback-widget-root');
  if (feedbackRoot) {
    var age = parseInt(feedbackRoot.getAttribute('data-account-age-days') || '-1', 10);
    var seen = false;
    try { seen = localStorage.getItem('openpanel_feedback_seen') === 'true'; } catch (e) {}
    if (age >= 3 && !seen) {
      var tpl = feedbackRoot.querySelector('template');
      if (tpl) { feedbackRoot.appendChild(tpl.content.cloneNode(true)); }
    }
  }
})();
"#;

/// The full-page shell: sidebar nav, topbar with the logged-in user and a
/// logout form, and an HTMX-swappable content region.
pub struct Shell<'a> {
    user: &'a str,
    csrf: &'a str,
    content: Markup,
    role: Role,
    path: &'a str,
    capabilities: CapabilitySet,
    theme: String,
    locale: String,
    timezone: String,
    account_age_days: i64,
}

impl<'a> Shell<'a> {
    /// Wrap rendered content in the panel chrome.
    pub fn new(user: &'a str, csrf: &'a str, content: Markup) -> Self {
        Self {
            user,
            csrf,
            content,
            role: Role::Owner,
            path: "/",
            capabilities: CapabilitySet::shipped(),
            theme: "dark".to_string(),
            locale: "en-US".to_string(),
            timezone: "UTC".to_string(),
            account_age_days: -1,
        }
    }

    /// Record the authenticated account's age in days for the
    /// feedback-widget gate. Negative when unknown (unauthenticated).
    pub fn with_account_age_days(mut self, days: i64) -> Self {
        self.account_age_days = days;
        self
    }

    /// Apply validated display preferences to document metadata.
    pub fn with_preferences(mut self, theme: &str, locale: &str, timezone: &str) -> Self {
        self.theme = theme.to_string();
        self.locale = locale.to_string();
        self.timezone = timezone.to_string();
        self
    }

    /// Attach the caller, canonical request path, and registered capabilities.
    pub fn with_navigation(
        mut self,
        role: Role,
        path: &'a str,
        capabilities: CapabilitySet,
    ) -> Self {
        self.role = role;
        self.path = path;
        self.capabilities = capabilities;
        self
    }

    /// Render the full document.
    pub fn render(&self) -> Markup {
        let current = self.current_item();
        let page_title = current.map_or("OpenPanel".to_string(), |item| {
            format!("{} · OpenPanel", item.label)
        });
        html! {
            (DOCTYPE)
            html lang=(self.locale) data-theme=(self.theme) data-timezone=(self.timezone) {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { (page_title) }
                    link rel="stylesheet" href="/assets/tokens.css";
                    link rel="stylesheet" href="/assets/app.css";
                    script src="/assets/htmx.min.js" defer {}
                }
                body {
                    div class="layout" {
                        details class="nav-disclosure" open {
                            summary { "Navigation" }
                            nav class="sidebar" aria-label="Primary navigation" {
                                button id="nav-rail-toggle" type="button" class="nav-rail-toggle"
                                    aria-pressed="false" aria-label="Toggle compact sidebar" title="Toggle compact sidebar" {
                                    span class="nav-rail-expand" aria-hidden="true" {
                                        svg class="nav-icon" viewBox="0 0 24 24" fill="none"
                                            stroke="currentColor" stroke-width="2"
                                            stroke-linecap="round" stroke-linejoin="round" {
                                            path d="M15 18l-6-6 6-6";
                                        }
                                    }
                                    span class="nav-rail-label" { "Compact" }
                                }
                                label class="nav-filter" {
                                    span class="visually-hidden" { "Filter menu" }
                                    input id="nav-filter" type="search"
                                        placeholder="Filter menu" autocomplete="off";
                                }
                                @for section in NAV_SECTIONS {
                                    @let visible = self.visible_items(section);
                                    @if !visible.is_empty() {
                                        details class="nav-section" data-section=(section.label) open {
                                            summary {
                                                span class="nav-section-label" { (section.label) }
                                            }
                                            ul class="nav-items" {
                                                @for item in visible {
                                                    @if self.is_active(item) {
                                                        li {
                                                            a class="nav-item" href=(item.href)
                                                                data-label=(item.label)
                                                                aria-current="page" hx-boost="true" {
                                                                (icon_svg(item.icon, item.label))
                                                                span class="nav-label" { (item.label) }
                                                            }
                                                        }
                                                    } @else {
                                                        li {
                                                            a class="nav-item" href=(item.href)
                                                                data-label=(item.label) hx-boost="true" {
                                                                (icon_svg(item.icon, item.label))
                                                                span class="nav-label" { (item.label) }
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
                        div class="main" {
                            header class="topbar" {
                                span class="user" { (self.user) }
                                form method="post" action="/logout" class="form form-inline" {
                                    (csrf_field(self.csrf))
                                    button type="submit" { "Log out" }
                                }
                            }
                            main id="content" {
                                nav class="breadcrumbs" aria-label="Breadcrumb" {
                                    a href="/" { "Dashboard" }
                                    @if let Some(item) = current {
                                        @if item.href != "/" {
                                            span aria-hidden="true" { " / " }
                                            span aria-current="page" { (item.label) }
                                        }
                                    }
                                }
                                (self.content)
                            }
                        }
                    }
                    div id="layer-root" aria-live="polite" {}
                    div id="form-errors" hx-swap-oob="true" aria-live="polite" {}
                    div id="feedback-widget-root" data-account-age-days=(self.account_age_days) {
                        @if crate::feedback::age_gate_passes(self.account_age_days) {
                            (crate::feedback::widget_template(self.csrf))
                        }
                    }
                    script { (maud::PreEscaped(NAV_JS)) }
                    script { (maud::PreEscaped(LAYER_JS)) }
                }
            }
        }
    }

    fn visible_items(&self, section: &NavSection) -> Vec<&NavItem> {
        section
            .items
            .iter()
            .filter(|item| {
                self.capabilities.contains(item.capability) && item.role.allows(self.role)
            })
            .collect()
    }

    fn is_active(&self, item: &NavItem) -> bool {
        if item.href == "/" {
            self.path == "/" || self.path == "/dashboard"
        } else {
            self.path == item.href || self.path.starts_with(&format!("{}/", item.href))
        }
    }

    fn current_item(&self) -> Option<&NavItem> {
        NAV_SECTIONS
            .iter()
            .flat_map(|section| section.items)
            .find(|item| self.is_active(item))
    }
}

#[cfg(test)]
mod tests {
    use openpanel_domain::Role;

    use super::*;

    #[test]
    fn shell_renders_nav_links_and_user() {
        let content = html! { p { "hello" } };
        let out = Shell::new("admin", "tok123", content)
            .render()
            .into_string();
        for item in NAV_SECTIONS.iter().flat_map(|section| section.items) {
            if CapabilitySet::shipped().contains(item.capability) {
                assert!(
                    out.contains(&format!("href=\"{}\"", item.href)),
                    "missing {}",
                    item.href
                );
                assert!(out.contains(item.label), "missing nav label {}", item.label);
            }
        }
        assert!(out.contains("admin"), "user shown in topbar");
        assert!(out.contains("Log out"), "logout button present");
        assert!(out.contains("htmx.min.js"), "htmx loaded");
    }

    #[test]
    fn every_form_includes_csrf_token() {
        // Forms rendered via the layout helpers (logout form + a content form
        // that uses the `csrf_field` helper) all carry a hidden _csrf input.
        let content = html! {
            form method="post" action="/x" {
                (csrf_field("tok123"))
                input type="text" name="a";
            }
        };
        let out = Shell::new("admin", "tok123", content)
            .render()
            .into_string();
        // every <form ...> block must contain a hidden _csrf input
        let mut rest = out.as_str();
        while let Some(start) = rest.find("<form") {
            let block = &rest[start..];
            let end = block.find("</form>").expect("form closes");
            let form_html = &block[..end];
            assert!(
                form_html.contains("name=\"_csrf\""),
                "form missing csrf: {form_html}"
            );
            rest = &block[end..];
        }
    }

    #[test]
    fn navigation_is_grouped_active_and_omits_unavailable_capabilities() {
        let out = Shell::new("admin", "tok123", html! { p { "sites" } })
            .with_navigation(Role::Owner, "/sites", CapabilitySet::shipped())
            .render()
            .into_string();

        for group in ["Overview", "Websites", "Operations", "System"] {
            assert!(out.contains(group), "missing group {group}: {out}");
        }
        for group in ["Mail &amp; Network", "Apps"] {
            assert!(!out.contains(group), "empty group rendered: {out}");
        }
        assert!(
            out.contains("href=\"/sites\" data-label=\"Sites\" aria-current=\"page\""),
            "sites active state: {out}"
        );
        assert!(out.contains("class=\"breadcrumbs\""), "breadcrumbs: {out}");
        assert!(
            !out.contains("href=\"/cron\""),
            "unregistered cron link: {out}"
        );
    }

    #[test]
    fn collapsible_sections_render_with_persistence_script() {
        let out = Shell::new("admin", "tok123", html! {})
            .with_navigation(Role::Owner, "/sites", CapabilitySet::shipped())
            .render()
            .into_string();
        assert!(
            out.contains("<details class=\"nav-section\" data-section=\"Operations\" open>"),
            "section disclosure missing: {out}"
        );
        assert!(
            out.contains("openpanel.nav.' + (s.getAttribute('data-section')"),
            "localStorage persistence script missing: {out}"
        );
    }

    #[test]
    fn nav_filter_and_rail_controls_render() {
        let out = Shell::new("admin", "tok123", html! {})
            .with_navigation(Role::Owner, "/", CapabilitySet::shipped())
            .render()
            .into_string();
        assert!(
            out.contains("id=\"nav-filter\""),
            "filter input missing: {out}"
        );
        assert!(
            out.contains("data-label=\"Dashboard\""),
            "filter labels missing: {out}"
        );
        assert!(
            out.contains("id=\"nav-rail-toggle\""),
            "rail toggle missing: {out}"
        );
        assert!(
            out.contains("openpanel.nav.rail"),
            "rail persistence: {out}"
        );
    }

    #[test]
    fn rail_toggle_has_accessible_name_and_expand_glyph() {
        let out = Shell::new("admin", "tok123", html! {})
            .with_navigation(Role::Owner, "/", CapabilitySet::shipped())
            .render()
            .into_string();
        assert!(
            out.contains("aria-label=\"Toggle compact sidebar\""),
            "toggle accessible name: {out}"
        );
        assert!(
            out.contains("class=\"nav-rail-expand\" aria-hidden=\"true\""),
            "expand glyph present and decorative: {out}"
        );
        assert!(
            out.contains("class=\"nav-rail-label\""),
            "compact label span: {out}"
        );
        assert!(
            out.contains("class=\"nav-section-label\""),
            "section headings wrapped for rail clipping: {out}"
        );
    }

    #[test]
    fn webmail_item_is_hidden_without_webmail_capability() {
        let out = Shell::new("alice", "tok123", html! {})
            .with_navigation(Role::User, "/", CapabilitySet::shipped())
            .render()
            .into_string();
        assert!(
            !out.contains("href=\"/webmail\""),
            "webmail link leaked without capability: {out}"
        );
    }

    #[test]
    fn navigation_hides_owner_administration_from_user() {
        let out = Shell::new("alice", "tok123", html! {})
            .with_navigation(Role::User, "/sites", CapabilitySet::shipped())
            .render()
            .into_string();

        assert!(!out.contains("href=\"/users\""), "users link leaked: {out}");
        assert!(
            !out.contains("href=\"/settings\""),
            "settings link leaked: {out}"
        );
        assert!(out.contains("href=\"/sites\""), "sites link missing: {out}");
    }

    #[test]
    fn navigation_has_keyboard_accessible_mobile_disclosure() {
        let out = Shell::new("admin", "tok123", html! {})
            .with_navigation(Role::Owner, "/", CapabilitySet::shipped())
            .render()
            .into_string();

        assert!(
            out.contains("class=\"nav-disclosure\""),
            "disclosure: {out}"
        );
        assert!(out.contains("summary"), "native keyboard control: {out}");
        assert!(out.contains("Navigation"), "accessible label: {out}");
    }

    #[test]
    fn shell_applies_validated_display_preferences() {
        let out = Shell::new("admin", "tok123", html! {})
            .with_preferences("light", "zh-CN", "Asia/Shanghai")
            .render()
            .into_string();
        assert!(out.contains("lang=\"zh-CN\""), "locale: {out}");
        assert!(out.contains("data-theme=\"light\""), "theme: {out}");
        assert!(
            out.contains("data-timezone=\"Asia/Shanghai\""),
            "timezone: {out}"
        );
    }

    #[test]
    fn shell_mounts_layer_root_and_form_errors() {
        let out = Shell::new("admin", "tok123", html! {})
            .with_account_age_days(-1)
            .render()
            .into_string();
        assert!(
            out.contains("<div id=\"layer-root\" aria-live=\"polite\">"),
            "layer root: {out}"
        );
        assert!(
            out.contains("id=\"form-errors\" hx-swap-oob=\"true\""),
            "form-errors container: {out}"
        );
        assert!(out.contains("htmx:afterRequest"), "toast hook wired: {out}");
    }

    #[test]
    fn shell_mounts_feedback_gate_without_template_for_new_accounts() {
        let out = Shell::new("admin", "tok123", html! {})
            .with_account_age_days(2)
            .render()
            .into_string();
        assert!(
            out.contains("id=\"feedback-widget-root\" data-account-age-days=\"2\""),
            "gate attribute: {out}"
        );
        assert!(
            !out.contains("op-feedback-widget"),
            "no widget for young account: {out}"
        );
    }

    #[test]
    fn shell_embeds_feedback_template_for_mature_accounts() {
        let out = Shell::new("admin", "tok123", html! {})
            .with_account_age_days(30)
            .render()
            .into_string();
        assert!(
            out.contains("data-account-age-days=\"30\""),
            "gate attribute: {out}"
        );
        assert!(
            out.contains("op-feedback-widget"),
            "widget template embedded: {out}"
        );
        assert!(
            out.contains("openpanel_feedback_seen"),
            "localStorage gate referenced: {out}"
        );
    }
}
