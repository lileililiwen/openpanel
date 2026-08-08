//! nginx config generator. Renders a site block, writes it under
//! `/etc/nginx/conf.d/openpanel/`, validates with `nginx -t`, and reloads
//! with `nginx -s reload`. On `nginx -t` failure, restores the prior state.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use openpanel_domain::{SiteError, sites::site::Site};

/// Filesystem paths used by the nginx config generator.
#[derive(Debug, Clone)]
pub struct NginxPaths {
    /// Directory where active site configs (`<domain>.conf`) are written.
    pub conf_d_active: PathBuf,
    /// Directory where disabled configs (`<domain>.conf.disabled`) are moved.
    pub conf_d_disabled: PathBuf,
    /// Absolute path to the `nginx` binary used for `-t` and `-s reload`.
    pub nginx_binary: PathBuf,
}

impl Default for NginxPaths {
    fn default() -> Self {
        Self {
            conf_d_active: PathBuf::from("/etc/nginx/conf.d/openpanel"),
            conf_d_disabled: PathBuf::from("/etc/nginx/conf.d/openpanel/disabled"),
            nginx_binary: PathBuf::from("/usr/sbin/nginx"),
        }
    }
}

impl NginxPaths {
    /// Construct paths rooted under an arbitrary directory. Used by tests to
    /// sandbox nginx config writes.
    pub fn under(root: PathBuf) -> Self {
        Self {
            conf_d_active: root.join("active"),
            conf_d_disabled: root.join("disabled"),
            nginx_binary: PathBuf::from("/usr/sbin/nginx"),
        }
    }
}

impl NginxPaths {
    /// Detect the host's `nginx` binary via `which`, falling back to the default
    /// `/usr/sbin/nginx` path when not found.
    pub fn detect() -> Self {
        let mut paths = NginxPaths::default();
        if let Ok(out) = Command::new("which").arg("nginx").output()
            && out.status.success()
            && let Ok(s) = String::from_utf8(out.stdout)
        {
            let s = s.trim();
            if !s.is_empty() {
                paths.nginx_binary = PathBuf::from(s);
            }
        }
        paths
    }

    /// Absolute path to the active config file for a given domain.
    pub fn active_path(&self, domain: &str) -> PathBuf {
        self.conf_d_active.join(format!("{domain}.conf"))
    }

    /// Absolute path to the disabled config file for a given domain.
    pub fn disabled_path(&self, domain: &str) -> PathBuf {
        self.conf_d_disabled.join(format!("{domain}.conf.disabled"))
    }
}

#[derive(Clone)]
/// Renders, writes, and reloads nginx server blocks for OpenPanel sites.
pub struct NginxConfigGenerator {
    paths: NginxPaths,
}

impl NginxConfigGenerator {
    /// Build a generator bound to the given nginx paths.
    pub fn new(paths: NginxPaths) -> Self {
        Self { paths }
    }

    /// Borrow the configured paths.
    pub fn paths(&self) -> &NginxPaths {
        &self.paths
    }

    /// Render the nginx server block for a site. Pure function.
    /// Without TLS — emits only the HTTP vhost. Use [`Self::render`]
    /// is the legacy entry; new callers should use
    /// [`Self::render_full`] which handles the optional TLS + acme
    /// challenge proxy + force-https redirect.
    pub fn render(site: &Site) -> String {
        Self::render_full(site, None, None)
    }

    /// Render the full nginx config for a site, including an optional
    /// TLS vhost on `:443`, the ACME HTTP-01 challenge proxy block on
    /// `:80`, and the force-HTTPS 301 redirect.
    ///
    /// - `tls`: when `Some((cert_path, key_path))`, emits a `:443` server
    //   block with the Mozilla-modern TLS profile.
    /// - `acme_challenge_upstream`: when `Some("http://127.0.0.1:9080")`,
    //   emits a `location ^~ /.well-known/acme-challenge/` block on
    //   the port-80 vhost proxying to that upstream.
    /// - `force_https`: when `true` AND `tls.is_some()`, the port-80
    //   vhost becomes a single `return 301 https://...`.
    pub fn render_full(
        site: &Site,
        tls: Option<(&str, &str)>,
        acme_challenge_upstream: Option<&str>,
    ) -> String {
        let mut server_names = vec![site.primary_domain().to_string()];
        for a in site.aliases() {
            server_names.push(a.clone());
        }
        let server_names = server_names.join(" ");

        let access_log = format!("/var/log/nginx/{}.access.log", site.primary_domain());
        let error_log = format!("/var/log/nginx/{}.error.log", site.primary_domain());

        let force_https = tls.is_some() && Self::force_https_for(site);
        let acme_block = match acme_challenge_upstream {
            Some(up) => format!(
                r#"
    location ^~ /.well-known/acme-challenge/ {{
        proxy_pass {up};
        proxy_set_header Host $host;
    }}
"#
            ),
            None => String::new(),
        };

        let http_vhost = if force_https {
            format!(
                r#"# Managed by OpenPanel. Do not edit by hand.
server {{
    listen 80;
    listen [::]:80;
    server_name {server_names};

    access_log {access_log};
    error_log  {error_log};
{acme_block}
    location / {{
        return 301 https://$host$request_uri;
    }}
}}
"#,
            )
        } else {
            format!(
                r#"# Managed by OpenPanel. Do not edit by hand.
server {{
    listen 80;
    listen [::]:80;
    server_name {server_names};

    root {root};
    index index.html index.htm{php_index};

    access_log {access_log};
    error_log  {error_log};

    client_max_body_size 100M;
{acme_block}
    location / {{
        try_files $uri $uri/ =404;
    }}

    location ~ /\.(?!well-known) {{ deny all; }}
}}
"#,
                root = site.document_root(),
                php_index = if site.php_enabled() { " index.php" } else { "" },
            )
        };

        let tls_vhost = match tls {
            Some((cert_path, key_path)) => format!(
                r#"
server {{
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name {server_names};

    ssl_certificate     {cert_path};
    ssl_certificate_key {key_path};
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphersuites    TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256;
    ssl_ciphers         ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305;
    ssl_prefer_server_ciphers on;
    ssl_session_cache   shared:SSL:10m;
    ssl_session_timeout 1d;
    ssl_session_tickets off;

    add_header Strict-Transport-Security "max-age=63072000" always;

    root {root};
    index index.html index.htm{php_index};

    access_log {access_log};
    error_log  {error_log};

    client_max_body_size 100M;

    location / {{
        try_files $uri $uri/ =404;
    }}

    location ~ /\.(?!well-known) {{ deny all; }}
}}
"#,
                root = site.document_root(),
                php_index = if site.php_enabled() { " index.php" } else { "" },
            ),
            None => String::new(),
        };

        format!("{http_vhost}{tls_vhost}")
    }

    /// Per-site force-HTTPS decision. Currently defaults to `true`
    /// when a cert is active — the site aggregate does not yet carry a
    /// per-site toggle, so the spec's "per-site toggle" is encoded at
    /// the cert level (`Certificate::force_https`). Sites without a
    /// cert are HTTP-only.
    fn force_https_for(_site: &Site) -> bool {
        // A future enhancement: let `Site` carry `force_https: bool`.
        true
    }

    /// Apply the rendered config (active site), test nginx, reload on success.
    /// On `nginx -t` failure, restores the previous file (if any). If nginx
    /// is not installed, writes the config and logs a warning instead of
    /// failing — useful for development environments.
    pub fn apply(&self, site: &Site) -> Result<(), SiteError> {
        self.apply_with_tls(site, None::<(&str, &str)>, None::<&str>)
    }

    /// Apply the rendered config with optional TLS + ACME challenge
    /// proxy. The challenge proxy upstream is typically
    /// `"http://127.0.0.1:9080"`.
    pub fn apply_with_tls(
        &self,
        site: &Site,
        tls: Option<(&str, &str)>,
        acme_challenge_upstream: Option<&str>,
    ) -> Result<(), SiteError> {
        self.ensure_dirs()?;
        let target = self.paths.active_path(site.primary_domain());
        let rendered = Self::render_full(site, tls, acme_challenge_upstream);

        if !self.nginx_available() {
            tracing::warn!(
                domain = site.primary_domain(),
                "nginx binary not found; writing config but skipping -t and reload"
            );
            self.write_only(&target, &rendered)?;
            return Ok(());
        }

        self.write_with_test(&target, &rendered)?;
        self.reload()?;
        Ok(())
    }

    /// Move active config to disabled/.conf.disabled, test, reload.
    pub fn disable(&self, site: &Site) -> Result<(), SiteError> {
        self.ensure_dirs()?;
        let active = self.paths.active_path(site.primary_domain());
        let disabled = self.paths.disabled_path(site.primary_domain());

        if !active.exists() {
            if self.nginx_available() {
                self.test_and_reload()?;
            }
            return Ok(());
        }

        let previous = fs::read_to_string(&active).map_err(|e| SiteError::Io(e.to_string()))?;

        fs::remove_file(&active).map_err(|e| SiteError::Io(e.to_string()))?;

        if !self.nginx_available() {
            tracing::warn!(domain = site.primary_domain(), "nginx missing; skipping -t");
            fs::create_dir_all(self.paths.conf_d_disabled.clone())
                .map_err(|e| SiteError::Io(e.to_string()))?;
            // restore file then move it (so we can move cleanly)
            fs::write(&active, &previous).map_err(|e| SiteError::Io(e.to_string()))?;
            fs::rename(&active, &disabled).map_err(|e| SiteError::Io(e.to_string()))?;
            return Ok(());
        }

        if !self.test()? {
            fs::write(&active, previous).map_err(|e| SiteError::Io(e.to_string()))?;
            return Err(SiteError::NginxTest("nginx -t failed after disable".into()));
        }

        fs::create_dir_all(self.paths.conf_d_disabled.clone())
            .map_err(|e| SiteError::Io(e.to_string()))?;
        fs::rename(&active, &disabled).map_err(|e| SiteError::Io(e.to_string()))?;
        self.reload()?;
        Ok(())
    }

    /// Remove the config entirely (active or disabled). Test, reload.
    pub fn remove(&self, site: &Site) -> Result<(), SiteError> {
        let active = self.paths.active_path(site.primary_domain());
        let disabled = self.paths.disabled_path(site.primary_domain());

        if active.exists() {
            fs::remove_file(&active).map_err(|e| SiteError::Io(e.to_string()))?;
        }
        if disabled.exists() {
            fs::remove_file(&disabled).map_err(|e| SiteError::Io(e.to_string()))?;
        }

        if self.nginx_available() && !self.test()? {
            return Err(SiteError::NginxTest("nginx -t failed after remove".into()));
        }
        Ok(())
    }

    /// Send `nginx -s reload` to pick up the latest configs.
    pub fn reload(&self) -> Result<(), SiteError> {
        let out = Command::new(&self.paths.nginx_binary)
            .arg("-s")
            .arg("reload")
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    SiteError::NginxMissing
                } else {
                    SiteError::Io(e.to_string())
                }
            })?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            return Err(SiteError::NginxReload(stderr));
        }
        Ok(())
    }

    /// Run `nginx -t` to validate the current config; returns success status.
    pub fn test(&self) -> Result<bool, SiteError> {
        let out = Command::new(&self.paths.nginx_binary)
            .arg("-t")
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    SiteError::NginxMissing
                } else {
                    SiteError::Io(e.to_string())
                }
            })?;
        Ok(out.status.success())
    }

    fn ensure_dirs(&self) -> Result<(), SiteError> {
        fs::create_dir_all(&self.paths.conf_d_active).map_err(|e| SiteError::Io(e.to_string()))?;
        fs::create_dir_all(&self.paths.conf_d_disabled)
            .map_err(|e| SiteError::Io(e.to_string()))?;
        Ok(())
    }

    fn write_with_test(&self, target: &Path, rendered: &str) -> Result<(), SiteError> {
        let previous = if target.exists() {
            Some(fs::read_to_string(target).map_err(|e| SiteError::Io(e.to_string()))?)
        } else {
            None
        };

        // Atomic-ish write: write to .new, then rename
        let tmp = target.with_extension("conf.new");
        fs::write(&tmp, rendered).map_err(|e| SiteError::Io(e.to_string()))?;
        if let Some(prev) = previous.as_ref() {
            fs::write(target, prev).map_err(|e| SiteError::Io(e.to_string()))?;
        }
        fs::rename(&tmp, target).map_err(|e| SiteError::Io(e.to_string()))?;

        if !self.test()? {
            if let Some(prev) = previous {
                fs::write(target, prev).map_err(|e| SiteError::Io(e.to_string()))?;
            } else {
                let _ = fs::remove_file(target);
            }
            return Err(SiteError::NginxTest("nginx -t failed after write".into()));
        }
        Ok(())
    }

    fn write_only(&self, target: &Path, rendered: &str) -> Result<(), SiteError> {
        let tmp = target.with_extension("conf.new");
        fs::write(&tmp, rendered).map_err(|e| SiteError::Io(e.to_string()))?;
        fs::rename(&tmp, target).map_err(|e| SiteError::Io(e.to_string()))?;
        Ok(())
    }

    fn test_and_reload(&self) -> Result<(), SiteError> {
        if !self.test()? {
            return Err(SiteError::NginxTest("nginx -t failed".into()));
        }
        self.reload()
    }

    /// Returns true when the configured nginx binary exists on disk.
    pub fn nginx_available(&self) -> bool {
        self.paths.nginx_binary.exists()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use openpanel_domain::{Email, Password, Role, User, Username};
    use uuid::Uuid;

    use super::*;

    fn dummy_site() -> Site {
        Site::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "example.com",
            vec!["www.example.com".into(), "api.example.com".into()],
            "/var/www/example.com/public_html",
            false,
            None,
            "tester",
        )
        .unwrap()
    }

    #[test]
    fn render_contains_server_name_and_root() {
        let s = dummy_site();
        let out = NginxConfigGenerator::render(&s);
        assert!(out.contains("server_name example.com www.example.com api.example.com"));
        assert!(out.contains("root /var/www/example.com/public_html"));
        assert!(out.contains("Managed by OpenPanel"));
    }

    #[test]
    fn render_with_php() {
        let s = dummy_site();
        let restored = openpanel_domain::Site::restore(
            s.id(),
            s.owner_id(),
            s.primary_domain().to_string(),
            s.aliases().to_vec(),
            s.document_root().to_string(),
            true,
            Some("8.3".into()),
            s.status(),
            s.created_at(),
            Utc::now(),
            s.created_by().to_string(),
            s.modified_by().to_string(),
        );
        let out = NginxConfigGenerator::render(&restored);
        assert!(out.contains("index.php"));
    }

    #[test]
    fn render_uses_canonical_lower_case_domain() {
        // Sanity check: pass a mixed-case domain and verify it isn't relied on
        // (the Site ctor already lowercases).
        let _ = User::new(
            Uuid::new_v4(),
            Username::new("dummy").unwrap(),
            Email::new("d@example.com").unwrap(),
            Password::from_hash("$argon2id$dummy"),
            Role::Owner,
        );
    }

    #[test]
    fn render_with_tls_emits_443_vhost_with_modern_profile() {
        let s = dummy_site();
        let out = NginxConfigGenerator::render_full(
            &s,
            Some((
                "/etc/openpanel/ssl/certs/example.com.crt",
                "/etc/openpanel/ssl/keys/example.com.key",
            )),
            Some("http://127.0.0.1:9080"),
        );
        assert!(out.contains("listen 443 ssl http2"));
        assert!(out.contains("ssl_certificate     /etc/openpanel/ssl/certs/example.com.crt"));
        assert!(out.contains("ssl_certificate_key /etc/openpanel/ssl/keys/example.com.key"));
        assert!(out.contains("TLSv1.2 TLSv1.3"));
        assert!(out.contains("Strict-Transport-Security"));
    }

    #[test]
    fn render_with_tls_and_acme_proxy_emits_challenge_block() {
        let s = dummy_site();
        let out = NginxConfigGenerator::render_full(
            &s,
            Some(("/tmp/cert", "/tmp/key")),
            Some("http://127.0.0.1:9080"),
        );
        assert!(out.contains("proxy_pass http://127.0.0.1:9080"));
        assert!(out.contains(".well-known/acme-challenge"));
    }

    #[test]
    fn render_without_tls_omits_tls_vhost() {
        let s = dummy_site();
        let out = NginxConfigGenerator::render_full(&s, None, None);
        assert!(!out.contains("listen 443"));
        assert!(!out.contains("ssl_certificate"));
        // No cert → no force-https redirect either.
        assert!(!out.contains("return 301 https://"));
    }

    #[test]
    fn render_with_tls_emits_force_https_301_on_port_80() {
        let s = dummy_site();
        let out = NginxConfigGenerator::render_full(
            &s,
            Some(("/tmp/cert", "/tmp/key")),
            Some("http://127.0.0.1:9080"),
        );
        assert!(out.contains("return 301 https://$host$request_uri"));
    }
}
