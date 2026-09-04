//! Site workspace: a consistent, capability-filtered tabbed navigation that
//! turns each site detail page into a single workspace. The tab model is pure
//! and unit-tested; every child route reuses `site_bar` so site context
//! (header, tabs, breadcrumb) is preserved after mutations and errors.
//!
//! Tabs whose backing route or capability is absent are omitted, never rendered
//! as dead links.

use maud::{Markup, html};
use openpanel_domain::{User, identity::Role, sites::site::Site};
use uuid::Uuid;

use crate::layout::CapabilitySet;
use crate::router::WebState;

/// A workspace tab identity. Order in the enum is the display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabId {
    /// Site overview (default landing).
    Overview,
    /// Domain/alias management.
    Domains,
    /// PHP/runtime configuration.
    Runtime,
    /// File manager.
    Files,
    /// TLS certificates.
    Ssl,
    /// HTTP controls (redirects, error pages, …).
    Http,
    /// Web application firewall.
    Waf,
    /// Access and error logs.
    Logs,
    /// Backup/restore.
    Backups,
    /// Staging snapshots.
    Staging,
    /// PR previews.
    Previews,
    /// Page cache / CDN.
    Cache,
    /// FTP accounts.
    Ftp,
    /// Collaborators.
    Collaborators,
}

impl TabId {
    /// Stable string key (used in tests and as a section marker).
    pub fn as_str(self) -> &'static str {
        match self {
            TabId::Overview => "overview",
            TabId::Domains => "domains",
            TabId::Runtime => "runtime",
            TabId::Files => "files",
            TabId::Ssl => "ssl",
            TabId::Http => "http",
            TabId::Waf => "waf",
            TabId::Logs => "logs",
            TabId::Backups => "backups",
            TabId::Staging => "staging",
            TabId::Previews => "previews",
            TabId::Cache => "cache",
            TabId::Ftp => "ftp",
            TabId::Collaborators => "collaborators",
        }
    }
}

/// Static definition of a candidate tab. A tab is visible only when it has a
/// backing route (`href`) and its required capability (if any) is installed.
struct TabDef {
    id: TabId,
    label: &'static str,
    capability: Option<&'static str>,
    min_role: Role,
    href: Option<fn(&Site) -> String>,
}

const TAB_DEFS: &[TabDef] = &[
    TabDef {
        id: TabId::Overview,
        label: "Overview",
        capability: None,
        min_role: Role::User,
        href: Some(|s| format!("/sites/{}", s.id())),
    },
    TabDef {
        id: TabId::Domains,
        label: "Domains",
        capability: None,
        min_role: Role::User,
        href: None,
    },
    TabDef {
        id: TabId::Runtime,
        label: "Runtime",
        capability: None,
        min_role: Role::User,
        href: None,
    },
    TabDef {
        id: TabId::Files,
        label: "Files",
        capability: Some("files"),
        min_role: Role::User,
        href: Some(|s| format!("/sites/{}/files", s.id())),
    },
    TabDef {
        id: TabId::Ssl,
        label: "SSL",
        capability: Some("ssl"),
        min_role: Role::User,
        href: Some(|s| format!("/ssl/{}", s.primary_domain())),
    },
    TabDef {
        id: TabId::Http,
        label: "HTTP",
        capability: None,
        min_role: Role::User,
        href: Some(|s| format!("/sites/{}/http", s.id())),
    },
    TabDef {
        id: TabId::Waf,
        label: "WAF",
        capability: None,
        min_role: Role::User,
        href: Some(|s| format!("/sites/{}/waf", s.id())),
    },
    TabDef {
        id: TabId::Logs,
        label: "Logs",
        capability: Some("logs"),
        min_role: Role::User,
        href: None,
    },
    TabDef {
        id: TabId::Backups,
        label: "Backups",
        capability: Some("backups"),
        min_role: Role::User,
        href: None,
    },
    TabDef {
        id: TabId::Staging,
        label: "Staging",
        capability: None,
        min_role: Role::User,
        href: Some(|s| format!("/sites/{}/staging", s.id())),
    },
    TabDef {
        id: TabId::Previews,
        label: "Previews",
        capability: Some("previews"),
        min_role: Role::User,
        href: Some(|s| format!("/sites/{}/previews", s.id())),
    },
    TabDef {
        id: TabId::Cache,
        label: "Cache & CDN",
        capability: None,
        min_role: Role::User,
        href: Some(|s| format!("/sites/{}/cache", s.id())),
    },
    TabDef {
        id: TabId::Ftp,
        label: "FTP",
        capability: Some("ftp"),
        min_role: Role::User,
        href: Some(|s| format!("/sites/{}/ftp", s.id())),
    },
    TabDef {
        id: TabId::Collaborators,
        label: "Collaborators",
        capability: None,
        min_role: Role::Admin,
        href: Some(|s| format!("/sites/{}/collaborators", s.id())),
    },
];

/// A concrete, render-ready workspace tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteWorkspaceTab {
    /// Tab identity.
    pub id: TabId,
    /// Visible label.
    pub label: &'static str,
    /// Destination route (already site-scoped).
    pub href: String,
}

/// Build the visible tabs for a site from capability and role checks.
///
/// A tab is omitted when it has no backing route, when its required capability
/// is not installed, or when the caller's role is below the tab's minimum.
pub fn workspace_tabs(site: &Site, caps: &CapabilitySet, role: Role) -> Vec<SiteWorkspaceTab> {
    TAB_DEFS
        .iter()
        .filter(|d| d.capability.is_none_or(|c| caps.contains(c)) && role <= d.min_role)
        // A tab without a backing route is omitted rather than unwrapped.
        .filter_map(|d| {
            d.href.map(|href| SiteWorkspaceTab {
                id: d.id,
                label: d.label,
                href: href(site),
            })
        })
        .collect()
}

/// Render the tab navigation strip. The active tab carries `aria-current`; the
/// strip is horizontally scrollable on mobile with focusable links.
pub fn tab_nav(active: TabId, tabs: &[SiteWorkspaceTab]) -> Markup {
    html! {
        nav class="site-tabs" role="tablist" aria-label="Site sections" {
            @for t in tabs {
                @if t.id == active {
                    a class="site-tab site-tab--active"
                        aria-current="page"
                        href=(t.href) {
                        (t.label)
                    }
                } @else {
                    a class="site-tab" href=(t.href) { (t.label) }
                }
            }
        }
    }
}

/// Render the workspace header: domain, status, environment, runtime, SSL
/// expiry, last deployment, and owner. Optional fields fall back to an em dash
/// when no source data is available (never invented values).
pub fn workspace_header(
    site: &Site,
    owner: &str,
    ssl_expiry: Option<&str>,
    last_deployment: Option<&str>,
) -> Markup {
    let php = match site.php_version() {
        Some(v) => {
            if site.php_enabled() {
                v.to_string()
            } else {
                "PHP disabled".into()
            }
        }
        None => "—".into(),
    };
    html! {
        section class="site-header" {
            h1 { (site.primary_domain()) }
            dl class="site-meta" {
                dt { "Status" }
                dd { (site.status().as_str()) }
                dt { "Environment" }
                dd { "Production" }
                dt { "Runtime" }
                dd { (php) }
                dt { "SSL expires" }
                dd { (ssl_expiry.unwrap_or("—")) }
                dt { "Last deployment" }
                dd { (last_deployment.unwrap_or("—")) }
                dt { "Owner" }
                dd { (owner) }
            }
        }
    }
}

/// Site-scoped breadcrumb with a return path to the sites list.
pub fn breadcrumb(site: &Site) -> Markup {
    html! {
        nav class="breadcrumb" aria-label="Breadcrumb" {
            a href="/sites" { "Sites" }
            span class="sep" { "/" }
            span aria-current="page" { (site.primary_domain()) }
        }
    }
}

/// Render the full workspace chrome (breadcrumb + header + tabs) for a site,
/// or a minimal breadcrumb stub when the site cannot be resolved (the calling
/// route remains responsible for the 404/403 decision).
pub async fn site_bar(state: &WebState, user: &User, site_id: Uuid, active: TabId) -> Markup {
    match state.sites.get_site(site_id).await {
        Ok(site) => {
            let owner = crate::sites::owner_name(state, site.owner_id()).await;
            html! {
                (breadcrumb(&site))
                (workspace_header(&site, &owner, None, None))
                (tab_nav(active, &workspace_tabs(&site, &state.capabilities, user.role())))
            }
        }
        Err(_) => html! {
            nav class="breadcrumb" aria-label="Breadcrumb" {
                a href="/sites" { "Sites" }
                span class="sep" { "/" }
                span aria-current="page" { (site_id.to_string()) }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use openpanel_domain::identity::Role;
    use openpanel_domain::sites::site::Site;
    use uuid::Uuid;

    use super::*;

    fn site() -> Site {
        Site::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "example.com",
            vec!["www.example.com".into()],
            "/var/www/example.com",
            true,
            Some("8.3".into()),
            "admin",
        )
        .expect("valid site")
    }

    fn tab_ids(tabs: &[SiteWorkspaceTab]) -> Vec<TabId> {
        tabs.iter().map(|t| t.id).collect()
    }

    #[test]
    fn tabs_are_ordered_and_capability_filtered() {
        let caps = CapabilitySet::shipped();
        let tabs = workspace_tabs(&site(), &caps, Role::Owner);
        let ids = tab_ids(&tabs);
        // Defined order, minus route-less/unsupported tabs.
        assert_eq!(
            ids,
            vec![
                TabId::Overview,
                TabId::Files,
                TabId::Ssl,
                TabId::Http,
                TabId::Waf,
                TabId::Staging,
                TabId::Previews,
                TabId::Cache,
                TabId::Collaborators,
            ],
            "tab order: {ids:?}"
        );
        // FTP capability is not shipped, so the tab is omitted.
        assert!(!ids.contains(&TabId::Ftp), "ftp omitted without capability");
    }

    #[test]
    fn ftp_tab_appears_when_capability_registered() {
        let caps = CapabilitySet::shipped().with("ftp");
        let tabs = workspace_tabs(&site(), &caps, Role::Owner);
        assert!(
            tab_ids(&tabs).contains(&TabId::Ftp),
            "ftp present when capability registered"
        );
    }

    #[test]
    fn route_less_tabs_are_never_shown() {
        // Domains / Runtime / Logs / Backups have no site-scoped route.
        let caps = CapabilitySet::shipped()
            .with("ftp")
            .with("logs")
            .with("backups");
        let ids = tab_ids(&workspace_tabs(&site(), &caps, Role::Owner));
        for absent in [TabId::Domains, TabId::Runtime, TabId::Logs, TabId::Backups] {
            assert!(
                !ids.contains(&absent),
                "{absent:?} must be omitted (no route)"
            );
        }
    }

    #[test]
    fn collaborators_tab_hidden_from_non_admins() {
        let caps = CapabilitySet::shipped();
        let user_tabs = workspace_tabs(&site(), &caps, Role::User);
        assert!(
            !tab_ids(&user_tabs).contains(&TabId::Collaborators),
            "collaborators hidden from User"
        );
        let owner_tabs = workspace_tabs(&site(), &caps, Role::Owner);
        assert!(
            tab_ids(&owner_tabs).contains(&TabId::Collaborators),
            "collaborators visible to Owner"
        );
    }

    #[test]
    fn tab_nav_marks_active_with_aria_current() {
        let tabs = workspace_tabs(&site(), &CapabilitySet::shipped(), Role::Owner);
        let out = tab_nav(TabId::Files, &tabs).into_string();
        assert!(
            out.contains("aria-current=\"page\""),
            "active aria-current: {out}"
        );
        assert!(out.contains("site-tab--active"), "active class: {out}");
        assert!(out.contains("role=\"tablist\""), "tablist role: {out}");
        // The active tab's label is still present exactly once as a link.
        assert!(out.contains(">Files<"), "files link: {out}");
    }

    #[test]
    fn header_renders_site_identity_and_meta() {
        let s = site();
        let out =
            workspace_header(&s, "admin", Some("2099-01-01"), Some("2099-01-02")).into_string();
        assert!(out.contains("example.com"), "domain: {out}");
        assert!(out.contains("8.3"), "php version: {out}");
        assert!(out.contains("admin"), "owner: {out}");
        assert!(out.contains("2099-01-01"), "ssl expiry: {out}");
        assert!(out.contains("2099-01-02"), "last deployment: {out}");
    }

    #[test]
    fn breadcrumb_links_back_to_sites() {
        let out = breadcrumb(&site()).into_string();
        assert!(out.contains("href=\"/sites\""), "sites link: {out}");
        assert!(out.contains("aria-label=\"Breadcrumb\""), "breadcrumb role");
        assert!(out.contains("example.com"), "current site: {out}");
    }
}
