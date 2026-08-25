//! Pure typed-controls to nginx-snippet renderer.

use openpanel_domain::site_http_controls::{
    IpEffect, RedirectStatus, SiteHttpControls, SiteHttpError,
};

/// Deterministic renderer for validated site HTTP controls.
pub struct SiteHttpControlsRenderer;

impl SiteHttpControlsRenderer {
    /// Compile a validated controls document into an nginx snippet.
    /// Output is byte-stable for equal input and never contains
    /// credential material (bcrypt hashes stay in htpasswd files).
    pub fn compile(
        controls: &SiteHttpControls,
        htpasswd_prefix: &str,
    ) -> Result<String, SiteHttpError> {
        if controls.is_empty() {
            return Ok(String::new());
        }
        let mut out = format!(
            "# openpanel-http-controls site={} rev={}\n",
            controls.site_id(),
            controls.version()
        );

        if let Some(policy) = controls.index_policy() {
            out.push_str(&format!("index {};\n", policy.order.join(" ")));
            out.push_str(&format!(
                "autoindex {};\n",
                if policy.autoindex { "on" } else { "off" }
            ));
        }

        for page in controls.error_pages() {
            out.push_str(&format!(
                "error_page {} {};\n",
                page.status, page.document_path
            ));
        }

        for rule in controls.redirects() {
            match rule.status {
                RedirectStatus::MovedPermanently | RedirectStatus::Found => {
                    let flag = match rule.status {
                        RedirectStatus::MovedPermanently => "permanent",
                        _ => "redirect",
                    };
                    out.push_str(&format!(
                        "rewrite ^{}(.*)$ {}$1 {flag};\n",
                        rule.source_prefix, rule.destination
                    ));
                }
                RedirectStatus::TemporaryRedirect | RedirectStatus::PermanentRedirect => {
                    out.push_str(&format!(
                        "location ^~ {} {{ return {} {}$request_uri; }}\n",
                        rule.source_prefix,
                        rule.status.code(),
                        rule.destination
                    ));
                }
            }
        }

        for (index, dir) in controls.protected_dirs().iter().enumerate() {
            let auth_file = format!("{htpasswd_prefix}/{index}.htpasswd");
            out.push_str(&format!(
                "location ^~ {} {{\n    auth_basic \"{}\";\n    auth_basic_user_file {auth_file};\n}}\n",
                dir.path_prefix, dir.realm
            ));
        }

        if let Some(policy) = controls.hotlink()
            && policy.default_deny
        {
            let referers = std::iter::once("none blocked server_names")
                .chain(policy.allowed_referers.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            out.push_str(&format!("valid_referers {referers};\n"));
            out.push_str("if ($invalid_referer) { return 403; }\n");
        }

        let has_allow = controls
            .ip_rules()
            .iter()
            .any(|rule| rule.effect == IpEffect::Allow);
        for rule in controls.ip_rules() {
            let verb = match rule.effect {
                IpEffect::Allow => "allow",
                IpEffect::Deny => "deny",
            };
            out.push_str(&format!("{verb} {};\n", rule.cidr));
        }
        if has_allow {
            out.push_str("deny all;\n");
        }

        for over in controls.mime_overrides() {
            out.push_str(&format!(
                "location ~* \\.{}$ {{ types {{ }} default_type {}; }}\n",
                over.extension, over.mime_type
            ));
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use openpanel_domain::site_http_controls::{
        BasicAuthAccount, ClientIpRule, ErrorPageOverride, HotlinkPolicy, IndexPolicy, IpEffect,
        MimeOverride, ProtectedDir, RedirectRule, RedirectStatus, SiteHttpControls,
    };

    use super::SiteHttpControlsRenderer;

    fn sample(redirects: Vec<RedirectRule>) -> SiteHttpControls {
        use openpanel_domain::site_http_controls::SiteHttpControlsInput;
        SiteHttpControls::new(
            uuid::Uuid::nil(),
            3,
            SiteHttpControlsInput {
                error_pages: vec![ErrorPageOverride {
                    status: 404,
                    document_path: "/errors/404.html".to_owned(),
                }],
                redirects,
                protected_dirs: vec![ProtectedDir {
                    path_prefix: "/admin".to_owned(),
                    realm: "admin area".to_owned(),
                    accounts: vec![BasicAuthAccount {
                        name: "root".to_owned(),
                        password_hash:
                            "$2b$12$ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFGHIJKLMNOPQR"
                                .to_owned(),
                    }],
                }],
                hotlink: Some(HotlinkPolicy {
                    allowed_referers: vec!["example.com".to_owned()],
                    default_deny: true,
                }),
                ip_rules: vec![
                    ClientIpRule {
                        ordinal: 1,
                        cidr: "10.0.0.0/8".to_owned(),
                        effect: IpEffect::Allow,
                    },
                    ClientIpRule {
                        ordinal: 2,
                        cidr: "192.0.2.1".to_owned(),
                        effect: IpEffect::Deny,
                    },
                ],
                mime_overrides: vec![MimeOverride {
                    extension: "webmanifest".to_owned(),
                    mime_type: "application/manifest+json".to_owned(),
                }],
                index_policy: Some(IndexPolicy {
                    order: vec!["index.php".to_owned(), "index.html".to_owned()],
                    autoindex: false,
                }),
            },
        )
        .unwrap()
    }

    #[test]
    fn test_render_is_deterministic_and_complete() {
        let controls = sample(vec![RedirectRule {
            ordinal: 1,
            source_prefix: "/old".to_owned(),
            destination: "/new".to_owned(),
            status: RedirectStatus::MovedPermanently,
        }]);
        let first =
            SiteHttpControlsRenderer::compile(&controls, "/etc/openpanel/http-auth/x").unwrap();
        let second =
            SiteHttpControlsRenderer::compile(&controls, "/etc/openpanel/http-auth/x").unwrap();
        assert_eq!(first, second);
        for needle in [
            "error_page 404 /errors/404.html;",
            "rewrite ^/old(.*)$ /new$1 permanent;",
            "auth_basic \"admin area\";",
            "auth_basic_user_file /etc/openpanel/http-auth/x/0.htpasswd;",
            "valid_referers none blocked server_names example.com;",
            "if ($invalid_referer) { return 403; }",
            "allow 10.0.0.0/8;",
            "deny 192.0.2.1;",
            "deny all;",
            "default_type application/manifest+json;",
            "index index.php index.html;",
            "autoindex off;",
        ] {
            assert!(first.contains(needle), "missing `{needle}` in:\n{first}");
        }
    }

    #[test]
    fn test_hash_never_reaches_snippet() {
        let controls = sample(Vec::new());
        let snippet = SiteHttpControlsRenderer::compile(&controls, "/auth").unwrap();
        assert!(!snippet.contains("$2b$"));
    }

    #[test]
    fn test_temporary_redirect_uses_return() {
        let controls = sample(vec![RedirectRule {
            ordinal: 1,
            source_prefix: "/tmp-src".to_owned(),
            destination: "https://elsewhere.example.com/landing".to_owned(),
            status: RedirectStatus::PermanentRedirect,
        }]);
        let snippet = SiteHttpControlsRenderer::compile(&controls, "/auth").unwrap();
        assert!(snippet.contains(
            "location ^~ /tmp-src { return 308 https://elsewhere.example.com/landing$request_uri; }"
        ));
    }

    #[test]
    fn test_empty_document_compiles_to_nothing() {
        let controls = SiteHttpControls::empty(uuid::Uuid::new_v4());
        let snippet = SiteHttpControlsRenderer::compile(&controls, "/auth").unwrap();
        assert!(snippet.is_empty());
    }
}
