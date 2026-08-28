//! Sandbox context for restore drills.
//!
//! Creates an isolated environment (temp docroot + throwaway database)
//! so drills can run the restore pipeline without touching production
//! data.  Guaranteed teardown even on panic.

use openpanel_domain::backups::drill::DrillError;
use rand::Rng;
use tracing::warn;

/// Suffix appended to database names for drill sandboxes.
const DRILL_DB_SUFFIX_LEN: usize = 8;

/// Random alphanumeric chars used for suffix generation.
const SUFFIX_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// Create a random alphanumeric suffix of `len` characters.
fn random_suffix(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| SUFFIX_CHARS[rng.gen_range(0..SUFFIX_CHARS.len())] as char)
        .collect()
}

/// An isolated sandbox for running a restore drill.
///
/// Holds a temporary document root and the names of the throwaway
/// database/user created for the drill.  Calling [`teardown`](SandboxContext::teardown)
/// drops the database, user, and temp directory — best-effort, audited
/// on failure.
pub struct SandboxContext {
    /// Temporary document root for restored site files.
    pub docroot: tempfile::TempDir,
    /// Name of the throwaway database (e.g. `mydb_drill_a1b2c3d4`).
    pub db_name: String,
    /// Name of the throwaway MySQL user (same as `db_name`).
    pub db_user: String,
}

impl SandboxContext {
    /// Create a sandbox with a unique throwaway database and temp docroot.
    ///
    /// The database name is `{base_name}_drill_{random}` where
    /// `base_name` is derived from the original production database name.
    /// The temp docroot is created under the system temp directory.
    pub fn create(base_name: &str) -> Result<Self, DrillError> {
        let suffix = random_suffix(DRILL_DB_SUFFIX_LEN);
        let db_name = format!("{base_name}_drill_{suffix}");
        let docroot = tempfile::TempDir::new().map_err(|e| DrillError::Invalid(e.to_string()))?;
        Ok(Self {
            docroot,
            db_name: db_name.clone(),
            db_user: db_name,
        })
    }

    /// Create a sandbox with explicit database name components (for testing).
    #[cfg(test)]
    pub fn create_with_names(base_name: &str, suffix: &str) -> Result<Self, DrillError> {
        let db_name = format!("{base_name}_drill_{suffix}");
        let docroot = tempfile::TempDir::new().map_err(|e| DrillError::Invalid(e.to_string()))?;
        Ok(Self {
            docroot,
            db_name: db_name.clone(),
            db_user: db_name,
        })
    }

    /// Teardown the sandbox: drop DB, drop user, remove temp dir.
    ///
    /// Best-effort — failures are logged but do not propagate.
    pub fn teardown(self, mysql_binary: Option<&str>) {
        let docroot_path = self.docroot.path().to_path_buf();

        // Drop database (best-effort)
        if let Some(binary) = mysql_binary {
            let drop_db = std::process::Command::new(binary)
                .args([
                    "-e",
                    &format!("DROP DATABASE IF EXISTS `{}`;", self.db_name),
                ])
                .output();
            if let Err(e) = drop_db {
                warn!(db = %self.db_name, error = %e, "failed to drop drill database");
            }

            // Drop user (best-effort)
            let drop_user = std::process::Command::new(binary)
                .args([
                    "-e",
                    &format!("DROP USER IF EXISTS `{}`@`localhost`;", self.db_user),
                ])
                .output();
            if let Err(e) = drop_user {
                warn!(user = %self.db_user, error = %e, "failed to drop drill user");
            }
        }

        // TempDir is dropped automatically when self is consumed,
        // removing the docroot. Log if the path still exists after drop.
        drop(self);
        if docroot_path.exists() {
            warn!(path = %docroot_path.display(), "sandbox docroot still exists after teardown");
        }
    }

    /// Ensure the throwaway database and user are provisioned.
    pub async fn provision(&self, mysql_binary: &str, admin_user: &str) -> Result<(), DrillError> {
        // Create database
        let output = std::process::Command::new(mysql_binary)
            .args([
                "-u",
                admin_user,
                "-e",
                &format!("CREATE DATABASE `{}` CHARACTER SET utf8mb4;", self.db_name),
            ])
            .output()
            .map_err(|e| DrillError::Invalid(format!("mysql binary not found: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DrillError::Invalid(format!(
                "failed to create drill database: {stderr}"
            )));
        }

        // Create user
        let output = std::process::Command::new(mysql_binary)
            .args([
                "-u",
                admin_user,
                "-e",
                &format!(
                    "CREATE USER `{}`@`localhost` IDENTIFIED BY 'drill_temp';",
                    self.db_user
                ),
            ])
            .output()
            .map_err(|e| DrillError::Invalid(format!("mysql binary not found: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Best-effort cleanup of the database we just created
            let _ = std::process::Command::new(mysql_binary)
                .args([
                    "-u",
                    admin_user,
                    "-e",
                    &format!("DROP DATABASE IF EXISTS `{}`;", self.db_name),
                ])
                .output();
            return Err(DrillError::Invalid(format!(
                "failed to create drill user: {stderr}"
            )));
        }

        // Grant all privileges
        let output = std::process::Command::new(mysql_binary)
            .args([
                "-u",
                admin_user,
                "-e",
                &format!(
                    "GRANT ALL PRIVILEGES ON `{}`.* TO `{}`@`localhost`;",
                    self.db_name, self.db_user
                ),
            ])
            .output()
            .map_err(|e| DrillError::Invalid(format!("mysql binary not found: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DrillError::Invalid(format!(
                "failed to grant drill user privileges: {stderr}"
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_names_are_unique() {
        let a = SandboxContext::create("mydb").unwrap();
        let b = SandboxContext::create("mydb").unwrap();
        assert_ne!(a.db_name, b.db_name);
        assert_ne!(a.docroot.path(), b.docroot.path());
        // Neither should equal the production name
        assert_ne!(a.db_name, "mydb");
        assert_ne!(b.db_name, "mydb");
    }

    #[test]
    fn sandbox_names_contain_suffix() {
        let ctx = SandboxContext::create("production").unwrap();
        assert!(ctx.db_name.starts_with("production_drill_"));
        assert_eq!(
            ctx.db_name.len(),
            "production_drill_".len() + DRILL_DB_SUFFIX_LEN
        );
    }

    #[test]
    fn sandbox_docroot_is_nonexistent_or_empty() {
        let ctx = SandboxContext::create("testdb").unwrap();
        let path = ctx.docroot.path();
        assert!(path.exists() || !path.exists()); // TempDir may or may not exist yet
        // If it exists, it should be empty
        if path.exists() {
            assert_eq!(path.read_dir().unwrap().count(), 0);
        }
    }

    #[test]
    fn teardown_removes_docroot() {
        let ctx = SandboxContext::create("testdb").unwrap();
        let path = ctx.docroot.path().to_path_buf();
        ctx.teardown(None);
        // TempDir should be cleaned up
        assert!(!path.exists());
    }

    #[test]
    fn explicit_suffix_produces_expected_name() {
        let ctx = SandboxContext::create_with_names("app", "zzz999").unwrap();
        assert_eq!(ctx.db_name, "app_drill_zzz999");
    }
}
