//! Navigation model: grouped sidebar sections, item metadata, and the
//! built-in inline-SVG icon set.
//!
//! The group map lives here (not in `layout.rs`) so the sidebar chrome
//! stays a pure renderer over a declarative model. Icon glyphs are
//! inline SVG (stroke style, 24×24 viewBox) served with the shell —
//! no external asset, no icon font, no JS framework.

use maud::{Markup, html};
use openpanel_domain::Role;

/// Minimum access level required to see a nav item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredRole {
    /// Any authenticated session.
    Authenticated,
    /// Owner role only.
    Owner,
}

impl RequiredRole {
    /// Whether the given role satisfies this requirement.
    pub fn allows(self, role: Role) -> bool {
        match self {
            Self::Authenticated => true,
            Self::Owner => matches!(role, Role::Owner),
        }
    }
}

/// A single navigation entry.
#[derive(Debug, Clone, Copy)]
pub struct NavItem {
    /// Canonical path the item links to.
    pub href: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    /// Runtime capability that gates visibility.
    pub capability: &'static str,
    /// Minimum role required to see the item.
    pub role: RequiredRole,
    /// Icon name from the built-in set (see [`icon_path`]).
    pub icon: &'static str,
}

/// A named group of navigation items.
#[derive(Debug, Clone, Copy)]
pub struct NavSection {
    /// Group heading.
    pub label: &'static str,
    /// Items in the group, in display order.
    pub items: &'static [NavItem],
}

const OVERVIEW: &[NavItem] = &[NavItem {
    href: "/",
    label: "Dashboard",
    capability: "dashboard",
    role: RequiredRole::Authenticated,
    icon: "dashboard",
}];

const WEBSITES: &[NavItem] = &[
    NavItem {
        href: "/sites",
        label: "Sites",
        capability: "sites",
        role: RequiredRole::Authenticated,
        icon: "globe",
    },
    NavItem {
        href: "/files",
        label: "Files",
        capability: "files",
        role: RequiredRole::Authenticated,
        icon: "folder",
    },
    NavItem {
        href: "/ssl",
        label: "SSL",
        capability: "ssl",
        role: RequiredRole::Authenticated,
        icon: "shield",
    },
    NavItem {
        href: "/databases",
        label: "Databases",
        capability: "databases",
        role: RequiredRole::Authenticated,
        icon: "database",
    },
];

const MAIL_NETWORK: &[NavItem] = &[
    NavItem {
        href: "/mail",
        label: "Mail",
        capability: "mail",
        role: RequiredRole::Authenticated,
        icon: "mail",
    },
    NavItem {
        href: "/webmail",
        label: "Webmail",
        capability: "webmail",
        role: RequiredRole::Authenticated,
        icon: "mailbox",
    },
    NavItem {
        href: "/dns",
        label: "DNS",
        capability: "dns",
        role: RequiredRole::Authenticated,
        icon: "network",
    },
    NavItem {
        href: "/security",
        label: "Security",
        capability: "host-security",
        role: RequiredRole::Owner,
        icon: "lock",
    },
];

const OPERATIONS: &[NavItem] = &[
    NavItem {
        href: "/monitoring",
        label: "Monitoring",
        capability: "monitoring",
        role: RequiredRole::Authenticated,
        icon: "activity",
    },
    NavItem {
        href: "/logs",
        label: "Logs",
        capability: "logs",
        role: RequiredRole::Authenticated,
        icon: "file-text",
    },
    NavItem {
        href: "/backups",
        label: "Backups",
        capability: "backups",
        role: RequiredRole::Authenticated,
        icon: "archive",
    },
    NavItem {
        href: "/previews",
        label: "Previews",
        capability: "previews",
        role: RequiredRole::Authenticated,
        icon: "git-pull-request",
    },
    NavItem {
        href: "/status-page",
        label: "Status page",
        capability: "status-page",
        role: RequiredRole::Authenticated,
        icon: "pulse",
    },
    NavItem {
        href: "/audit",
        label: "Audit",
        capability: "audit",
        role: RequiredRole::Owner,
        icon: "list",
    },
    NavItem {
        href: "/cron",
        label: "Cron",
        capability: "cron",
        role: RequiredRole::Authenticated,
        icon: "clock",
    },
    NavItem {
        href: "/services",
        label: "Services",
        capability: "system-services",
        role: RequiredRole::Authenticated,
        icon: "server",
    },
];

const APPS: &[NavItem] = &[
    NavItem {
        href: "/software",
        label: "Software Center",
        capability: "software-center",
        role: RequiredRole::Owner,
        icon: "grid",
    },
    NavItem {
        href: "/marketplace",
        label: "Plugin Marketplace",
        capability: "marketplace",
        role: RequiredRole::Owner,
        icon: "store",
    },
    NavItem {
        href: "/plugins",
        label: "Plugins",
        capability: "plugins",
        role: RequiredRole::Owner,
        icon: "plug",
    },
];

const SYSTEM: &[NavItem] = &[
    NavItem {
        href: "/docker",
        label: "Containers",
        capability: "docker",
        role: RequiredRole::Owner,
        icon: "box",
    },
    NavItem {
        href: "/container/quota",
        label: "Container quota",
        capability: "docker",
        role: RequiredRole::Owner,
        icon: "gauge",
    },
    NavItem {
        href: "/registry",
        label: "Container Registry",
        capability: "container-registry",
        role: RequiredRole::Owner,
        icon: "package",
    },
    NavItem {
        href: "/registry/credentials",
        label: "Registry credentials",
        capability: "docker",
        role: RequiredRole::Owner,
        icon: "key",
    },
    NavItem {
        href: "/users",
        label: "Users",
        capability: "users",
        role: RequiredRole::Owner,
        icon: "users",
    },
    NavItem {
        href: "/admin/branding",
        label: "Branding",
        capability: "themeable-ui",
        role: RequiredRole::Owner,
        icon: "palette",
    },
    NavItem {
        href: "/settings",
        label: "Settings",
        capability: "settings",
        role: RequiredRole::Owner,
        icon: "sliders",
    },
];

/// The six-section sidebar group map, in fixed display order.
pub const NAV_SECTIONS: &[NavSection] = &[
    NavSection {
        label: "Overview",
        items: OVERVIEW,
    },
    NavSection {
        label: "Websites",
        items: WEBSITES,
    },
    NavSection {
        label: "Mail & Network",
        items: MAIL_NETWORK,
    },
    NavSection {
        label: "Operations",
        items: OPERATIONS,
    },
    NavSection {
        label: "Apps",
        items: APPS,
    },
    NavSection {
        label: "System",
        items: SYSTEM,
    },
];

/// Resolve an icon name to its inline-SVG `<path d="…">` data.
///
/// Returns `None` for unknown names so the "every item has an icon"
/// test can fail loudly instead of silently shipping a blank glyph.
pub fn icon_path(name: &str) -> Option<&'static str> {
    Some(match name {
        "dashboard" => "M3 3h7v9H3zM14 3h7v5h-7zM14 12h7v9h-7zM3 16h7v5H3z",
        "globe" => {
            "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zM2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"
        }
        "folder" => "M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z",
        "shield" => "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
        "database" => {
            "M21 5c0 1.66-4 3-9 3S3 6.66 3 5s4-3 9-3 9 1.34 9 3zM3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5M3 12c0 1.66 4 3 9 3s9-1.34 9-3"
        }
        "mail" => {
            "M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM22 6l-10 7L2 6"
        }
        "mailbox" => "M6 21V4h14l-4 4 4 4H6",
        "network" => {
            "M12 8v6M7 17l4-3M17 17l-4-3M12 5a3 3 0 1 0 0-.01M5 19a3 3 0 1 0 0-.01M19 19a3 3 0 1 0 0-.01"
        }
        "lock" => {
            "M7 11V7a5 5 0 0 1 10 0v4M5 11h14a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2z"
        }
        "activity" => "M22 12h-4l-3 9L9 3l-3 9H2",
        "file-text" => {
            "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6M16 13H8M16 17H8"
        }
        "archive" => {
            "M4 3h16a1 1 0 0 1 1 1v3a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM5 8v11a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8M10 12h4"
        }
        "clock" => "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zM12 6v6l4 2",
        "server" => {
            "M4 4h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM4 14h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2zM7 6h.01M7 18h.01"
        }
        "box" => {
            "M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16zM3.3 7l8.7 5 8.7-5M12 22V12"
        }
        "gauge" => "M12 14l4-4M3.34 19a10 10 0 1 1 17.32 0",
        "package" => {
            "M16.5 9.4L7.55 4.24M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16zM3.3 7l8.7 5 8.7-5M12 22V12"
        }
        "key" => "M7.5 15.5a5.5 5.5 0 1 0 0-.01M21 2l-9.6 9.6M15.5 7.5l3 3L22 7l-3-3",
        "grid" => "M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z",
        "users" => {
            "M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM23 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75"
        }
        "palette" => {
            "M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.93 0 1.65-.75 1.65-1.69 0-.44-.18-.84-.44-1.13-.29-.29-.44-.65-.44-1.13A1.64 1.64 0 0 1 14.4 16h2c3.05 0 5.56-2.5 5.56-5.55C21.96 6.01 17.46 2 12 2zM7 12.01h.01M10 8.01h.01M14 8.01h.01M17 12.01h.01"
        }
        "sliders" => "M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6",
        "plug" => "M12 22v-5M9 8V2M15 8V2M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8z",
        "store" => "M3 9l1-5h16l1 5M5 9v11h14V9M9 20v-6h6v6M3 9h18",
        "git-pull-request" => {
            "M6 9V3a2 2 0 1 1 4 0v6a2 2 0 1 1-4 0zM6 9v12M18 21v-6a3 3 0 0 0-3-3h-3M15 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4z"
        }
        "list" => "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
        "pulse" => "M22 12h-4l-3 9L9 3l-3 9H2",
        _ => return None,
    })
}

/// Render the inline SVG icon for a nav item.
///
/// The glyph is decorative (`aria-hidden`) because the visible label
/// text renders next to it; the `<title>` is kept for the spec's
/// accessible-title requirement and native tooltip behaviour.
pub fn icon_svg(name: &str, label: &str) -> Markup {
    let path = icon_path(name).unwrap_or_default();
    html! {
        svg class="nav-icon" viewBox="0 0 24 24" fill="none"
            stroke="currentColor" stroke-width="2"
            stroke-linecap="round" stroke-linejoin="round"
            aria-hidden="true" focusable="false" {
            title { (label) }
            path d=(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_model_item_appears_in_exactly_one_section() {
        let mut hrefs: Vec<&str> = NAV_SECTIONS
            .iter()
            .flat_map(|section| section.items)
            .map(|item| item.href)
            .collect();
        let count = hrefs.len();
        hrefs.sort_unstable();
        hrefs.dedup();
        assert_eq!(hrefs.len(), count, "duplicate nav href across sections");
    }

    #[test]
    fn nav_model_section_order_is_fixed() {
        let labels: Vec<&str> = NAV_SECTIONS.iter().map(|s| s.label).collect();
        assert_eq!(
            labels,
            vec![
                "Overview",
                "Websites",
                "Mail & Network",
                "Operations",
                "Apps",
                "System",
            ]
        );
    }

    #[test]
    fn every_nav_item_has_a_builtin_icon() {
        for section in NAV_SECTIONS {
            for item in section.items {
                assert!(
                    icon_path(item.icon).is_some(),
                    "item `{}` references unknown icon `{}`",
                    item.label,
                    item.icon
                );
            }
        }
    }

    #[test]
    fn icon_svg_renders_title_matching_label() {
        let out = icon_svg("globe", "Sites").into_string();
        assert!(out.contains("<title>Sites</title>"), "title missing: {out}");
        assert!(out.contains("class=\"nav-icon\""), "icon class: {out}");
    }

    #[test]
    fn every_nav_item_capability_is_registered() {
        // The shell hides nav items whose capability is absent from the
        // shipped inventory; regression guard against the original bug where
        // mail/dns/cron/backups/logs/security/etc. were undiscoverable.
        let shipped = crate::layout::CapabilitySet::shipped();
        for section in NAV_SECTIONS {
            for item in section.items {
                assert!(
                    shipped.contains(item.capability),
                    "nav item `{}` capability `{}` is not in shipped()",
                    item.label,
                    item.capability
                );
            }
        }
    }

    #[test]
    fn every_shipped_capability_has_a_nav_item() {
        // Drift guard: a capability in the shipped inventory must correspond
        // to a real navigation entry, otherwise it is registered but hidden.
        let shipped = crate::layout::CapabilitySet::shipped();
        let nav_caps: Vec<&str> = NAV_SECTIONS
            .iter()
            .flat_map(|section| section.items)
            .map(|item| item.capability)
            .collect();
        for cap in shipped.iter() {
            assert!(
                nav_caps.contains(&cap),
                "shipped capability `{cap}` has no navigation item"
            );
        }
    }
}
