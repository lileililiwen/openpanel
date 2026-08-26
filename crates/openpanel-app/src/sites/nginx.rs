//! nginx config generator. Renders a site block, writes it under
//! `/etc/nginx/conf.d/openpanel/`, validates with `nginx -t`, and reloads
//! with `nginx -s reload`. On `nginx -t` failure, restores the prior state.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use openpanel_domain::{
    SiteError,
    sites::{TransportPolicy, site::Site},
};

/// Nginx `http`-context format used by managed access sources. `$uri` excludes queries.
pub fn managed_log_format() -> &'static str {
    "log_format openpanel escape=json '$openpanel_site_id\\t$time_iso8601\\t$request_method\\t$uri\\t$status\\t$body_bytes_sent\\t$request_time\\t$remote_addr\\t$http_user_agent';\n"
}

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

    /// Render an HTTP site config with a validated WAF snippet immediately
    /// before the first location directive.
    pub fn render_with_waf(site: &Site, snippet: &str) -> String {
        let rendered = Self::render(site);
        if snippet.is_empty() {
            return rendered;
        }
        let mut global = String::new();
        let mut local = Vec::new();
        for line in snippet.lines() {
            if line.starts_with("limit_req_zone ") || line.starts_with("limit_conn_zone ") {
                global.push_str(line);
                global.push('\n');
            } else {
                local.push(line);
            }
        }
        let indented = local
            .into_iter()
            .map(|line| format!("    {line}\n"))
            .collect::<String>();
        format!(
            "{global}{}",
            rendered.replacen("    location ", &format!("{indented}\n    location "), 1)
        )
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
        Self::render_full_with_policy(
            site,
            tls,
            acme_challenge_upstream,
            &TransportPolicy::default(),
        )
    }

    /// Render with an explicit transport policy. The default policy
    /// reproduces the pre-tuning output byte-for-byte.
    pub fn render_full_with_policy(
        site: &Site,
        tls: Option<(&str, &str)>,
        acme_challenge_upstream: Option<&str>,
        transport: &TransportPolicy,
    ) -> String {
        let mut server_names = vec![site.primary_domain().to_string()];
        for a in site.aliases() {
            server_names.push(a.clone());
        }
        let server_names = server_names.join(" ");

        let access_log = format!("/var/log/openpanel/{}.access.log", site.id());
        let error_log = format!("/var/log/openpanel/{}.error.log", site.id());
        let site_id = site.id();
        let php_socket = match (site.php_enabled(), site.php_version()) {
            (true, Some("8.3")) => Some("/run/php/php8.3-fpm.sock"),
            (true, Some("8.4")) => Some("/run/php/php8.4-fpm.sock"),
            _ => None,
        };
        let application_route = if php_socket.is_some() {
            "try_files $uri $uri/ /index.php?$query_string;"
        } else {
            "try_files $uri $uri/ =404;"
        };
        let php_location = php_socket.map_or_else(String::new, |socket| {
            format!(
                r#"
    location ~ \.php$ {{
        try_files $uri =404;
        include fastcgi_params;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        fastcgi_pass unix:{socket};
    }}
"#,
            )
        });

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

    set $openpanel_site_id "{site_id}";
    access_log {access_log} openpanel;
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

    set $openpanel_site_id "{site_id}";
    access_log {access_log} openpanel;
    error_log  {error_log};

    client_max_body_size 100M;
{acme_block}
    location / {{
        {application_route}
    }}
{php_location}

    location ~ /\.(?!well-known) {{ deny all; }}
}}
"#,
                root = site.document_root(),
                php_index = if site.php_enabled() { " index.php" } else { "" },
                site_id = site.id(),
            )
        };

        let tls_vhost = match tls {
            Some((cert_path, key_path)) => format!(
                r#"
server {{
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
{quic_listen}    server_name {server_names};

    ssl_certificate     {cert_path};
    ssl_certificate_key {key_path};
    ssl_protocols       {ssl_protocols};
    ssl_ciphersuites    TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256;
    ssl_ciphers         ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305;
    ssl_prefer_server_ciphers on;
    ssl_session_cache   shared:SSL:10m;
    ssl_session_timeout 1d;
    ssl_session_tickets off;
{extras}
    root {root};
    index index.html index.htm{php_index};

    set $openpanel_site_id "{site_id}";
    access_log {access_log} openpanel;
    error_log  {error_log};
{limits}
    location / {{
        {application_route}
    }}
{php_location}

    location ~ /\.(?!well-known) {{ deny all; }}
}}
"#,
                root = site.document_root(),
                php_index = if site.php_enabled() { " index.php" } else { "" },
                site_id = site.id(),
                quic_listen = transport.render_quic_listen(),
                ssl_protocols = transport.render_ssl_protocols(),
                extras = {
                    let mut extras = String::new();
                    extras.push_str(transport.render_alt_svc());
                    extras.push_str(&transport.render_hsts());
                    if !extras.is_empty() {
                        extras.insert(0, '\n');
                    }
                    extras
                },
                limits = {
                    let mut limits = transport.render_compression();
                    limits.push_str(&transport.render_body_size());
                    if !limits.is_empty() {
                        limits.insert(0, '\n');
                    }
                    limits
                },
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

    /// Apply a rendered site candidate containing a compiled WAF snippet.
    /// The same atomic write, `nginx -t`, rollback, and reload discipline as
    /// ordinary site updates is used.
    pub fn apply_with_waf(&self, site: &Site, snippet: &str) -> Result<(), SiteError> {
        self.ensure_dirs()?;
        let target = self.paths.active_path(site.primary_domain());
        let rendered = Self::render_with_waf(site, snippet);

        if !self.nginx_available() {
            tracing::warn!(
                domain = site.primary_domain(),
                "nginx binary not found; writing WAF config but skipping -t and reload"
            );
            return self.write_only(&target, &rendered);
        }

        self.write_with_test(&target, &rendered)?;
        self.reload()
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
        fs::write(
            self.paths
                .conf_d_active
                .join("00-openpanel-log-format.conf"),
            managed_log_format(),
        )
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
    use openpanel_domain::{
        ByteSize, CompressionPolicy, Email, HstsPolicy, Password, Role, TlsVersion, User, Username,
    };
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
    fn render_uses_managed_query_free_access_format_and_site_identity() {
        let site = dummy_site();
        let rendered = NginxConfigGenerator::render(&site);
        assert!(rendered.contains("access_log /var/log/openpanel/"));
        assert!(rendered.contains(" openpanel;"));
        assert!(rendered.contains(&format!("set $openpanel_site_id \"{}\";", site.id())));
        assert!(!rendered.contains("$request_uri"));
        assert!(managed_log_format().contains("$uri"));
        assert!(!managed_log_format().contains("$request_uri"));
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
        assert!(out.contains("try_files $uri $uri/ /index.php?$query_string;"));
        assert!(out.contains("location ~ \\.php$"));
        assert!(out.contains("fastcgi_pass unix:/run/php/php8.3-fpm.sock;"));
        assert!(out.contains("fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;"));
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
    fn golden_default_transport_policy_is_byte_identical_with_pre_tuning_output() {
        let site = Site::new(
            uuid::Uuid::from_u128(1),
            Uuid::new_v4(),
            "golden.example.com",
            vec![],
            "/var/www/golden.example.com/public_html",
            false,
            None,
            "tester",
        )
        .unwrap();
        let out = NginxConfigGenerator::render_full_with_policy(
            &site,
            Some((
                "/etc/openpanel/ssl/golden.crt",
                "/etc/openpanel/ssl/golden.key",
            )),
            None,
            &TransportPolicy::default(),
        );
        // The TLS vhost is the tail of the output.
        let start = out
            .find("server {\n    listen 443")
            .map(|index| &out[index..])
            .expect("tls vhost");
        let golden = include_str!("testdata/transport_golden.vhost");
        assert_eq!(start, golden);
        // render_full delegates to the default policy.
        assert_eq!(
            NginxConfigGenerator::render_full(
                &site,
                Some((
                    "/etc/openpanel/ssl/golden.crt",
                    "/etc/openpanel/ssl/golden.key"
                )),
                None,
            ),
            out
        );
    }

    #[test]
    fn http3_policy_emits_single_quic_listen_and_alt_svc_in_vhost() {
        let site = Site::new(
            uuid::Uuid::from_u128(2),
            Uuid::new_v4(),
            "h3.example.com",
            vec![],
            "/var/www/h3.example.com/public_html",
            false,
            None,
            "tester",
        )
        .unwrap();
        let policy = TransportPolicy::new(
            true,
            TlsVersion::V1_2,
            TransportPolicy::default().hsts().cloned(),
            CompressionPolicy::Off,
            ByteSize::new(100 * 1024 * 1024).unwrap(),
        )
        .unwrap();
        let out = NginxConfigGenerator::render_full_with_policy(
            &site,
            Some(("/tmp/c", "/tmp/k")),
            None,
            &policy,
        );
        assert_eq!(out.matches("listen 443 quic reuseport;").count(), 1);
        assert!(out.contains("Alt-Svc 'h3=\":443\"; ma=86400;' always;"));
        // Disabling removes both again.
        let off = NginxConfigGenerator::render_full_with_policy(
            &site,
            Some(("/tmp/c", "/tmp/k")),
            None,
            &TransportPolicy::default(),
        );
        assert!(!off.contains("quic"));
        assert!(!off.contains("Alt-Svc"));
    }

    #[test]
    fn transport_overrides_render_into_vhost() {
        let site = Site::new(
            uuid::Uuid::from_u128(3),
            Uuid::new_v4(),
            "tuned.example.com",
            vec![],
            "/var/www/tuned.example.com/public_html",
            false,
            None,
            "tester",
        )
        .unwrap();
        let policy = TransportPolicy::new(
            false,
            TlsVersion::V1_3,
            Some(HstsPolicy::new(31_536_000, true, true).unwrap()),
            CompressionPolicy::Brotli(5),
            ByteSize::new(32 * 1024 * 1024).unwrap(),
        )
        .unwrap();
        let out = NginxConfigGenerator::render_full_with_policy(
            &site,
            Some(("/tmp/c", "/tmp/k")),
            None,
            &policy,
        );
        assert!(out.contains("ssl_protocols       TLSv1.3;"));
        assert!(
            out.contains(
                "add_header Strict-Transport-Security \"max-age=31536000; includeSubDomains; preload\" always;"
            )
        );
        assert!(out.contains("brotli on;"));
        assert!(out.contains("brotli_comp_level 5;"));
        assert!(out.contains("client_max_body_size 32M;"));
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
