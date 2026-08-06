//! Document root provisioning: create `/var/www/<domain>/public_html`
//! with mode 0755, a placeholder `index.html`, and chown to the site
//! owner (best-effort; requires the openpanel process to run as root).

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use openpanel_domain::SiteError;

pub struct DocumentRootProvisioner;

impl DocumentRootProvisioner {
    pub fn provision(root: &Path, owner_name: &str, site_name: &str) -> Result<(), SiteError> {
        if !root.exists() {
            fs::create_dir_all(root).map_err(|e| SiteError::Io(e.to_string()))?;
        }
        fs::set_permissions(root, fs::Permissions::from_mode(0o755))
            .map_err(|e| SiteError::Io(e.to_string()))?;

        let index = root.join("index.html");
        if !index.exists() {
            let mut f = fs::File::create(&index).map_err(|e| SiteError::Io(e.to_string()))?;
            let body = format!(
                r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>{site_name}</title></head>
<body>
<h1>{site_name}</h1>
<p>Site provisioned by OpenPanel. Owner: <strong>{owner_name}</strong>.</p>
<p>Replace this file with your own content.</p>
</body>
</html>
"#
            );
            f.write_all(body.as_bytes())
                .map_err(|e| SiteError::Io(e.to_string()))?;
        }

        Ok(())
    }

    /// Best-effort chown via /etc/passwd lookup. Falls back silently when
    /// the user does not exist or the openpanel process is unprivileged.
    pub fn try_chown(path: &Path, owner_name: &str) -> Result<(), SiteError> {
        if let Some((uid, gid)) = lookup_passwd(owner_name) {
            std::os::unix::fs::chown(path, Some(uid), Some(gid))
                .map_err(|e| SiteError::Io(e.to_string()))?;
        }
        Ok(())
    }
}

/// Parse `/etc/passwd` to find a user's UID and primary GID. Returns
/// `None` if the file is missing or the user is not found.
fn lookup_passwd(name: &str) -> Option<(u32, u32)> {
    let f = fs::File::open("/etc/passwd").ok()?;
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(7, ':');
        let user = parts.next()?;
        let _ = parts.next()?; // password
        let uid: u32 = parts.next()?.parse().ok()?;
        let gid: u32 = parts.next()?.parse().ok()?;
        if user == name {
            return Some((uid, gid));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_root_succeeds() {
        let (uid, _gid) = lookup_passwd("root").expect("root user must exist on Linux");
        assert_eq!(uid, 0);
    }

    #[test]
    fn lookup_missing_user_returns_none() {
        assert!(lookup_passwd("__openpanel_no_such_user__").is_none());
    }
}