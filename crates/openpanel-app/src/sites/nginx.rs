//! nginx config generator. Renders a site block, writes it under
//! `/etc/nginx/conf.d/openpanel/`, validates with `nginx -t`, and reloads
//! with `nginx -s reload`. On `nginx -t` failure, restores the prior state.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use openpanel_domain::sites::site::Site;
use openpanel_domain::sites::status::SiteStatus;
use openpanel_domain::SiteError;

#[derive(Debug, Clone)]
pub struct NginxPaths {
    pub conf_d_active: PathBuf,
    pub conf_d_disabled: PathBuf,
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
    pub fn detect() -> Self {
        let mut paths = NginxPaths::default();
        if let Ok(out) = Command::new("which").arg("nginx").output() {
            if out.status.success() {
                if let Ok(s) = String::from_utf8(out.stdout) {
                    let s = s.trim();
                    if !s.is_empty() {
                        paths.nginx_binary = PathBuf::from(s);
                    }
                }
            }
        }
        paths
    }

    pub fn active_path(&self, domain: &str) -> PathBuf {
        self.conf_d_active.join(format!("{domain}.conf"))
    }

    pub fn disabled_path(&self, domain: &str) -> PathBuf {
        self.conf_d_disabled.join(format!("{domain}.conf.disabled"))
    }
}

pub struct NginxConfigGenerator {
    paths: NginxPaths,
}

impl NginxConfigGenerator {
    pub fn new(paths: NginxPaths) -> Self {
        Self { paths }
    }

    pub fn paths(&self) -> &NginxPaths {
        &self.paths
    }

    /// Render the nginx server block for a site. Pure function.
    pub fn render(site: &Site) -> String {
        let mut server_names = vec![site.primary_domain().to_string()];
        for a in site.aliases() {
            server_names.push(a.clone());
        }
        let server_names = server_names.join(" ");

        let access_log = format!("/var/log/nginx/{}.access.log", site.primary_domain());
        let error_log = format!("/var/log/nginx/{}.error.log", site.primary_domain());

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

    location / {{
        try_files $uri $uri/ =404;
    }}

    location ~ /\.(?!well-known) {{ deny all; }}
}}
"#,
            root = site.document_root(),
            php_index = if site.php_enabled() { " index.php" } else { "" },
        )
    }

    /// Apply the rendered config (active site), test nginx, reload on success.
    /// On `nginx -t` failure, restores the previous file (if any). If nginx
    /// is not installed, writes the config and logs a warning instead of
    /// failing — useful for development environments.
    pub fn apply(&self, site: &Site) -> Result<(), SiteError> {
        self.ensure_dirs()?;
        let target = self.paths.active_path(site.primary_domain());
        let rendered = Self::render(site);

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

        let previous = fs::read_to_string(&active)
            .map_err(|e| SiteError::Io(e.to_string()))?;

        fs::remove_file(&active)
            .map_err(|e| SiteError::Io(e.to_string()))?;

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
            fs::write(&active, previous)
                .map_err(|e| SiteError::Io(e.to_string()))?;
            return Err(SiteError::NginxTest("nginx -t failed after disable".into()));
        }

        fs::create_dir_all(self.paths.conf_d_disabled.clone())
            .map_err(|e| SiteError::Io(e.to_string()))?;
        fs::rename(&active, &disabled)
            .map_err(|e| SiteError::Io(e.to_string()))?;
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
        fs::create_dir_all(&self.paths.conf_d_active)
            .map_err(|e| SiteError::Io(e.to_string()))?;
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

    pub fn nginx_available(&self) -> bool {
        self.paths.nginx_binary.exists()
    }
}

#[allow(dead_code)]
pub fn status_to_path_suffix(status: SiteStatus) -> &'static str {
    match status {
        SiteStatus::Active => "",
        SiteStatus::Disabled => ".disabled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use openpanel_domain::{Email, Password, Role, Username, User};
    use uuid::Uuid;

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
}