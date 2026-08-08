//! Shared HTML chrome: the HTMX shell (sidebar + topbar + content region)
//! plus the `csrf_field` helper used by every state-changing form.

use maud::{DOCTYPE, Markup, html};

/// Navigation entries shown in the sidebar as `(href, label)`.
pub const NAV_LINKS: &[(&str, &str)] = &[
    ("/", "Dashboard"),
    ("/sites", "Sites"),
    ("/databases", "Databases"),
    ("/files", "Files"),
    ("/ssl", "SSL"),
    ("/monitoring", "Monitoring"),
    ("/users", "Users"),
];

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
}

impl<'a> Shell<'a> {
    /// Wrap rendered content in the panel chrome.
    pub fn new(user: &'a str, csrf: &'a str, content: Markup) -> Self {
        Self {
            user,
            csrf,
            content,
        }
    }

    /// Render the full document.
    pub fn render(&self) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en" {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { "OpenPanel" }
                    link rel="stylesheet" href="/assets/app.css";
                    script src="/assets/htmx.min.js" defer {}
                }
                body {
                    div class="layout" {
                        nav class="sidebar" {
                            @for (href, label) in NAV_LINKS {
                                a href=(href) hx-boost="true" { (label) }
                            }
                        }
                        div class="main" {
                            header class="topbar" {
                                span class="user" { (self.user) }
                                form method="post" action="/logout" {
                                    (csrf_field(self.csrf))
                                    button type="submit" { "Log out" }
                                }
                            }
                            main id="content" {
                                (self.content)
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_renders_nav_links_and_user() {
        let content = html! { p { "hello" } };
        let out = Shell::new("admin", "tok123", content)
            .render()
            .into_string();
        for (href, label) in NAV_LINKS {
            assert!(out.contains(&format!("href=\"{href}\"")), "missing {href}");
            assert!(out.contains(label), "missing nav label {label}");
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
}
