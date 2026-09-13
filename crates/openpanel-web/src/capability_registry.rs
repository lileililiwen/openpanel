//! Capability navigation registry: one discoverability inventory for every
//! mounted first-class web workflow.
//!
//! The shell sidebar (`nav_model`), the capability inventory (`layout`),
//! and the site workspace tabs (`site_workspace`) previously kept separate
//! lists. This module is the single metadata source they all query: each
//! entry declares its route, capability, label, icon, scope, and minimum
//! role. Router mounting stays explicit in `router.rs`; authorization stays
//! in handlers/services. This registry is metadata only.
//!
//! Covers `capability-navigation`: One Discoverability Inventory,
//! Navigation and Router Agree, Site Workspace Is Complete and Scoped,
//! Role Filtering Is Defense in Depth.

use maud::{Markup, html};
use openpanel_domain::identity::Role;

/// Whether an entry is a top-level shell destination or lives inside a
/// site workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteScope {
    /// Top-level shell destination (sidebar navigation).
    Global,
    /// Site-scoped destination (workspace tab).
    Site,
}

/// One discoverability inventory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteMeta {
    /// Stable entry key (unique across the registry).
    pub key: &'static str,
    /// Canonical route. Site entries use `{id}` / `{domain}` / `{site_id}`
    /// placeholders for the site segment.
    pub route: &'static str,
    /// Human-readable label (matches the nav item / tab label).
    pub label: &'static str,
    /// Icon name from the built-in set (see `nav_model::icon_path`).
    /// Site entries reuse the global icon vocabulary.
    pub icon: &'static str,
    /// Runtime capability that gates visibility.
    pub capability: &'static str,
    /// Minimum role required to see the entry.
    pub min_role: Role,
    /// Global (sidebar) or site-scoped (workspace tab).
    pub scope: RouteScope,
    /// Workspace tab key (`TabId::as_str`) for site entries; `None` for
    /// global entries.
    pub site_tab: Option<&'static str>,
}

/// The single discoverability inventory.
///
/// Global entries mirror `NAV_SECTIONS` (one entry per sidebar item);
/// site entries mirror the workspace tabs (one entry per tab, including
/// the previously route-less Domains, Runtime, Logs, and Backups tabs).
pub const REGISTRY: &[RouteMeta] = &[
    // Global entries (sidebar, in display order).
    RouteMeta {
        key: "dashboard",
        route: "/",
        label: "Dashboard",
        icon: "dashboard",
        capability: "dashboard",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "sites",
        route: "/sites",
        label: "Sites",
        icon: "globe",
        capability: "sites",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "files",
        route: "/files",
        label: "Files",
        icon: "folder",
        capability: "files",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "ssl",
        route: "/ssl",
        label: "SSL",
        icon: "shield",
        capability: "ssl",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "databases",
        route: "/databases",
        label: "Databases",
        icon: "database",
        capability: "databases",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "mail",
        route: "/mail",
        label: "Mail",
        icon: "mail",
        capability: "mail",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "webmail",
        route: "/webmail",
        label: "Webmail",
        icon: "mailbox",
        capability: "webmail",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "dns",
        route: "/dns",
        label: "DNS",
        icon: "network",
        capability: "dns",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "security",
        route: "/security",
        label: "Security",
        icon: "lock",
        capability: "host-security",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "security-findings",
        route: "/security/findings",
        label: "Security findings",
        icon: "shield",
        capability: "host-security",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "monitoring",
        route: "/monitoring",
        label: "Monitoring",
        icon: "activity",
        capability: "monitoring",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "logs",
        route: "/logs",
        label: "Logs",
        icon: "file-text",
        capability: "logs",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "backups",
        route: "/backups",
        label: "Backups",
        icon: "archive",
        capability: "backups",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "previews",
        route: "/previews",
        label: "Previews",
        icon: "git-pull-request",
        capability: "previews",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "status-page",
        route: "/status-page",
        label: "Status page",
        icon: "pulse",
        capability: "status-page",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "audit",
        route: "/audit",
        label: "Audit",
        icon: "list",
        capability: "audit",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "cron",
        route: "/cron",
        label: "Cron",
        icon: "clock",
        capability: "cron",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "services",
        route: "/services",
        label: "Services",
        icon: "server",
        capability: "system-services",
        min_role: Role::User,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "software",
        route: "/software",
        label: "Software Center",
        icon: "grid",
        capability: "software-center",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "marketplace",
        route: "/marketplace",
        label: "Plugin Marketplace",
        icon: "store",
        capability: "marketplace",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "plugins",
        route: "/plugins",
        label: "Plugins",
        icon: "plug",
        capability: "plugins",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "docker",
        route: "/docker",
        label: "Containers",
        icon: "box",
        capability: "docker",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "container-quota",
        route: "/container/quota",
        label: "Container quota",
        icon: "gauge",
        capability: "docker",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "registry",
        route: "/registry",
        label: "Container Registry",
        icon: "package",
        capability: "container-registry",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "registry-credentials",
        route: "/registry/credentials",
        label: "Registry credentials",
        icon: "key",
        capability: "docker",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "users",
        route: "/users",
        label: "Users",
        icon: "users",
        capability: "users",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "branding",
        route: "/admin/branding",
        label: "Branding",
        icon: "palette",
        capability: "themeable-ui",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    RouteMeta {
        key: "settings",
        route: "/settings",
        label: "Settings",
        icon: "sliders",
        capability: "settings",
        min_role: Role::Owner,
        scope: RouteScope::Global,
        site_tab: None,
    },
    // Site entries (workspace tabs, in tab order).
    RouteMeta {
        key: "site-overview",
        route: "/sites/{id}",
        label: "Overview",
        icon: "globe",
        capability: "sites",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("overview"),
    },
    RouteMeta {
        key: "site-domains",
        route: "/sites/{id}/domains",
        label: "Domains",
        icon: "globe",
        capability: "sites",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("domains"),
    },
    RouteMeta {
        key: "site-runtime",
        route: "/sites/{id}/runtime",
        label: "Runtime",
        icon: "server",
        capability: "sites",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("runtime"),
    },
    RouteMeta {
        key: "site-files",
        route: "/sites/{site_id}/files",
        label: "Files",
        icon: "folder",
        capability: "files",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("files"),
    },
    RouteMeta {
        key: "site-ssl",
        route: "/ssl/{domain}",
        label: "SSL",
        icon: "shield",
        capability: "ssl",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("ssl"),
    },
    RouteMeta {
        key: "site-http",
        route: "/sites/{id}/http",
        label: "HTTP",
        icon: "sliders",
        capability: "sites",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("http"),
    },
    RouteMeta {
        key: "site-waf",
        route: "/sites/{id}/waf",
        label: "WAF",
        icon: "shield",
        capability: "sites",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("waf"),
    },
    RouteMeta {
        key: "site-logs",
        route: "/sites/{id}/logs",
        label: "Logs",
        icon: "file-text",
        capability: "logs",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("logs"),
    },
    RouteMeta {
        key: "site-backups",
        route: "/sites/{id}/backups",
        label: "Backups",
        icon: "archive",
        capability: "backups",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("backups"),
    },
    RouteMeta {
        key: "site-staging",
        route: "/sites/{id}/staging",
        label: "Staging",
        icon: "git-pull-request",
        capability: "sites",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("staging"),
    },
    RouteMeta {
        key: "site-previews",
        route: "/sites/{id}/previews",
        label: "Previews",
        icon: "git-pull-request",
        capability: "previews",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("previews"),
    },
    RouteMeta {
        key: "site-cache",
        route: "/sites/{id}/cache",
        label: "Cache & CDN",
        icon: "package",
        capability: "sites",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("cache"),
    },
    RouteMeta {
        key: "site-ftp",
        route: "/sites/{id}/ftp",
        label: "FTP",
        icon: "folder",
        capability: "ftp",
        min_role: Role::User,
        scope: RouteScope::Site,
        site_tab: Some("ftp"),
    },
    RouteMeta {
        key: "site-collaborators",
        route: "/sites/{id}/collaborators",
        label: "Collaborators",
        icon: "users",
        capability: "sites",
        min_role: Role::Admin,
        scope: RouteScope::Site,
        site_tab: Some("collaborators"),
    },
];

/// All registry entries.
pub fn entries() -> &'static [RouteMeta] {
    REGISTRY
}

/// Global (sidebar) entries.
pub fn global_entries() -> impl Iterator<Item = &'static RouteMeta> {
    REGISTRY.iter().filter(|e| e.scope == RouteScope::Global)
}

/// Site-scoped (workspace tab) entries.
pub fn site_entries() -> impl Iterator<Item = &'static RouteMeta> {
    REGISTRY.iter().filter(|e| e.scope == RouteScope::Site)
}

/// Deduplicated capability keys for the global entries, in registry order.
///
/// This is the shipped inventory the shell derives navigation from: what is
/// installed/mounted, not who may see it (role gating stays in the shell).
pub fn global_capabilities() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for entry in global_entries() {
        if !out.contains(&entry.capability) {
            out.push(entry.capability);
        }
    }
    out
}

/// Find an entry by its canonical route.
pub fn find_by_route(route: &str) -> Option<&'static RouteMeta> {
    REGISTRY.iter().find(|e| e.route == route)
}

/// Find the first entry for a capability key.
pub fn find_by_capability(capability: &str) -> Option<&'static RouteMeta> {
    REGISTRY.iter().find(|e| e.capability == capability)
}

/// Find the site entry for a workspace tab key (`TabId::as_str`).
pub fn site_entry_for_tab(tab: &str) -> Option<&'static RouteMeta> {
    site_entries().find(|e| e.site_tab == Some(tab))
}

/// Whether a role may see an entry (defense in depth: the shell hides the
/// link AND the handler still denies direct access).
pub fn role_may_see(role: Role, entry: &RouteMeta) -> bool {
    role <= entry.min_role
}

/// Whether a concrete request path matches a route template. `{id}`,
/// `{domain}`, and `{site_id}` segments match any single non-empty segment.
pub fn template_matches(template: &str, path: &str) -> bool {
    let mut t_segs = template.split('/').peekable();
    let mut p_segs = path.split('/').peekable();
    loop {
        match (t_segs.next(), p_segs.next()) {
            (None, None) => return true,
            (Some(_), None) | (None, Some(_)) => return false,
            (Some(t), Some(p)) => {
                if t == p {
                    continue;
                }
                let is_placeholder = t == "{id}" || t == "{domain}" || t == "{site_id}";
                if is_placeholder && !p.is_empty() {
                    continue;
                }
                return false;
            }
        }
    }
}

/// Explicit unavailable state: the capability is not installed/mounted, so
/// the workflow cannot be offered. Reuses the `op-empty-state` vocabulary
/// (no new CSS tokens) with copy that names the missing capability.
pub fn unavailable_state(capability: &str) -> Markup {
    html! {
        section class="op-empty-state" aria-live="polite" {
            h2 { "Not available" }
            p { "This workflow is not enabled on this panel." }
            p { "Missing capability: " code { (capability) } }
            a class="btn" href="/sites" { "Back to Sites" }
        }
    }
}

/// Explicit unauthorized state: the caller is authenticated but below the
/// entry's minimum role. Reuses the `op-error-state` vocabulary (no new CSS
/// tokens); handlers still return the authorization response.
pub fn unauthorized_state() -> Markup {
    crate::ui_states::ErrorState::new(
        "Not authorized",
        "Your role does not permit this workflow.",
        "/",
    )
    .render()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Capability under test: `capability-navigation`.
    const CAPABILITY: &str = "capability-navigation";

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "capability-navigation");
        assert!(!REGISTRY.is_empty(), "registry must not be empty");
    }

    #[test]
    fn registry_keys_routes_and_tabs_are_unique() {
        let mut keys: Vec<&str> = REGISTRY.iter().map(|e| e.key).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "duplicate registry key");

        let mut routes: Vec<&str> = REGISTRY.iter().map(|e| e.route).collect();
        let route_count = routes.len();
        routes.sort_unstable();
        routes.dedup();
        assert_eq!(routes.len(), route_count, "duplicate registry route");

        let mut tabs: Vec<&str> = site_entries().filter_map(|e| e.site_tab).collect();
        let tab_count = tabs.len();
        tabs.sort_unstable();
        tabs.dedup();
        assert_eq!(tabs.len(), tab_count, "duplicate site tab key");
    }

    #[test]
    fn registry_entries_have_icons_labels_roles_and_scope() {
        for entry in REGISTRY {
            assert!(!entry.label.is_empty(), "empty label: {}", entry.key);
            assert!(!entry.icon.is_empty(), "empty icon: {}", entry.key);
            assert!(
                !entry.capability.is_empty(),
                "empty capability: {}",
                entry.key
            );
            assert!(
                entry.route.starts_with('/'),
                "route must start with /: {}",
                entry.key
            );
            match entry.scope {
                RouteScope::Global => assert!(
                    entry.site_tab.is_none(),
                    "global entry must not name a tab: {}",
                    entry.key
                ),
                RouteScope::Site => assert!(
                    entry.site_tab.is_some(),
                    "site entry must name a tab: {}",
                    entry.key
                ),
            }
        }
    }

    #[test]
    fn registry_icons_come_from_builtin_set() {
        for entry in REGISTRY {
            assert!(
                crate::nav_model::icon_path(entry.icon).is_some(),
                "entry `{}` references unknown icon `{}`",
                entry.key,
                entry.icon
            );
        }
    }

    #[test]
    fn global_capabilities_cover_shipped_inventory() {
        let caps = global_capabilities();
        for expected in [
            "dashboard",
            "sites",
            "files",
            "databases",
            "ssl",
            "mail",
            "webmail",
            "dns",
            "host-security",
            "monitoring",
            "logs",
            "backups",
            "previews",
            "status-page",
            "cron",
            "system-services",
            "software-center",
            "marketplace",
            "plugins",
            "docker",
            "container-registry",
            "users",
            "themeable-ui",
            "settings",
            "audit",
        ] {
            assert!(
                caps.contains(&expected),
                "shipped capability `{expected}` missing from registry"
            );
        }
    }

    #[test]
    fn role_filtering_is_defense_in_depth() {
        let owner_only = find_by_route("/audit").expect("audit entry");
        assert!(!role_may_see(Role::User, owner_only));
        assert!(role_may_see(Role::Owner, owner_only));
        let authed = find_by_route("/sites").expect("sites entry");
        assert!(role_may_see(Role::User, authed));
        let collab = find_by_route("/sites/{id}/collaborators").expect("collab entry");
        assert!(!role_may_see(Role::User, collab));
        assert!(role_may_see(Role::Admin, collab));
    }

    #[test]
    fn template_matching_covers_site_segments() {
        assert!(template_matches(
            "/sites/{id}/domains",
            "/sites/abc/domains"
        ));
        assert!(template_matches(
            "/sites/{site_id}/files",
            "/sites/abc/files"
        ));
        assert!(template_matches("/ssl/{domain}", "/ssl/example.com"));
        assert!(!template_matches(
            "/sites/{id}/domains",
            "/sites/abc/runtime"
        ));
        assert!(!template_matches(
            "/sites/{id}/domains",
            "/sites/abc/domains/x"
        ));
        assert!(!template_matches("/sites", "/sites/abc"));
    }

    #[test]
    fn unavailable_and_unauthorized_states_are_explicit() {
        let out = unavailable_state("logs").into_string();
        assert!(out.contains("Not available"), "title: {out}");
        assert!(out.contains("logs"), "capability named: {out}");
        assert!(out.contains("op-empty-state"), "stable class: {out}");
        let denied = unauthorized_state().into_string();
        assert!(denied.contains("Not authorized"), "title: {denied}");
        assert!(denied.contains("op-error-state"), "stable class: {denied}");
    }
}
