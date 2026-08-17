//! Shared HTML chrome: the HTMX shell (sidebar + topbar + content region)
//! plus the `csrf_field` helper used by every state-changing form.

use std::collections::BTreeSet;

use maud::{DOCTYPE, Markup, html};
use openpanel_domain::Role;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequiredRole {
    Authenticated,
    Owner,
}

impl RequiredRole {
    fn allows(self, role: Role) -> bool {
        match self {
            Self::Authenticated => true,
            Self::Owner => matches!(role, Role::Owner),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct NavItem {
    href: &'static str,
    label: &'static str,
    capability: &'static str,
    role: RequiredRole,
}

#[derive(Debug, Clone, Copy)]
struct NavSection {
    label: &'static str,
    items: &'static [NavItem],
}

const OVERVIEW: &[NavItem] = &[NavItem {
    href: "/",
    label: "Dashboard",
    capability: "dashboard",
    role: RequiredRole::Authenticated,
}];
const HOSTING: &[NavItem] = &[
    NavItem {
        href: "/sites",
        label: "Sites",
        capability: "sites",
        role: RequiredRole::Authenticated,
    },
    NavItem {
        href: "/files",
        label: "Files",
        capability: "files",
        role: RequiredRole::Authenticated,
    },
    NavItem {
        href: "/databases",
        label: "Databases",
        capability: "databases",
        role: RequiredRole::Authenticated,
    },
    NavItem {
        href: "/ssl",
        label: "SSL",
        capability: "ssl",
        role: RequiredRole::Authenticated,
    },
    NavItem {
        href: "/webmail",
        label: "Webmail",
        capability: "webmail",
        role: RequiredRole::Authenticated,
    },
];
const OPERATIONS: &[NavItem] = &[
    NavItem {
        href: "/monitoring",
        label: "Monitoring",
        capability: "monitoring",
        role: RequiredRole::Authenticated,
    },
    NavItem {
        href: "/logs",
        label: "Logs",
        capability: "logs",
        role: RequiredRole::Authenticated,
    },
    NavItem {
        href: "/backups",
        label: "Backups",
        capability: "backups",
        role: RequiredRole::Authenticated,
    },
    NavItem {
        href: "/cron",
        label: "Cron",
        capability: "cron",
        role: RequiredRole::Authenticated,
    },
    NavItem {
        href: "/services",
        label: "Services",
        capability: "system-services",
        role: RequiredRole::Authenticated,
    },
];
const SECURITY_NETWORK: &[NavItem] = &[
    NavItem {
        href: "/security",
        label: "Security",
        capability: "host-security",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/dns",
        label: "DNS",
        capability: "dns",
        role: RequiredRole::Authenticated,
    },
    NavItem {
        href: "/mail",
        label: "Mail",
        capability: "mail",
        role: RequiredRole::Authenticated,
    },
];
const ADMINISTRATION: &[NavItem] = &[
    NavItem {
        href: "/docker",
        label: "Containers",
        capability: "docker",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/container/quota",
        label: "Container quota",
        capability: "docker",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/registry/credentials",
        label: "Registry credentials",
        capability: "docker",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/software",
        label: "Software Center",
        capability: "software-center",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/users",
        label: "Users",
        capability: "users",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/registry",
        label: "Container Registry",
        capability: "container-registry",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/plugins",
        label: "Plugins",
        capability: "plugins",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/marketplace",
        label: "Plugin Marketplace",
        capability: "marketplace",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/audit",
        label: "Audit log",
        capability: "audit",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/admin/branding",
        label: "Branding",
        capability: "themeable-ui",
        role: RequiredRole::Owner,
    },
    NavItem {
        href: "/settings",
        label: "Settings",
        capability: "settings",
        role: RequiredRole::Owner,
    },
];
const NAV_SECTIONS: &[NavSection] = &[
    NavSection {
        label: "Overview",
        items: OVERVIEW,
    },
    NavSection {
        label: "Hosting",
        items: HOSTING,
    },
    NavSection {
        label: "Operations",
        items: OPERATIONS,
    },
    NavSection {
        label: "Security & Network",
        items: SECURITY_NETWORK,
    },
    NavSection {
        label: "Administration",
        items: ADMINISTRATION,
    },
];

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

/// Render a hidden `_csrf` input carrying the current session's token.
/// Every state-changing web form MUST include this field.
pub fn csrf_field(token: &str) -> Markup {
    html! {
        input type="hidden" name="_csrf" value=(token);
    }
}

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
        }
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
                                @for section in NAV_SECTIONS {
                                    @let visible = self.visible_items(section);
                                    @if !visible.is_empty() {
                                        section class="nav-section" {
                                            h2 { (section.label) }
                                            @for item in visible {
                                                @if self.is_active(item) {
                                                    a href=(item.href) aria-current="page" hx-boost="true" { (item.label) }
                                                } @else {
                                                    a href=(item.href) hx-boost="true" { (item.label) }
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

        for group in ["Overview", "Hosting", "Operations", "Administration"] {
            assert!(out.contains(group), "missing group {group}: {out}");
        }
        assert!(
            !out.contains("Security &amp; Network"),
            "empty group rendered: {out}"
        );
        assert!(
            out.contains("href=\"/sites\" aria-current=\"page\""),
            "sites active state: {out}"
        );
        assert!(out.contains("class=\"breadcrumbs\""), "breadcrumbs: {out}");
        assert!(
            !out.contains("href=\"/cron\""),
            "unregistered cron link: {out}"
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
}
