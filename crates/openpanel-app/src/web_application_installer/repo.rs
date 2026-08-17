//! SQLite-backed adapter for the web-application-installer
//! bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    IdempotencyKey, InstallArtifact, InstallDb, InstallOverlay, InstallPlan, InstallRun,
    InstalledWebApp, WebApplicationInstallerError, WebApplicationInstallerRepository,
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// SQLite-backed repository.
#[derive(Clone)]
pub struct SqliteWebApplicationInstallerRepository {
    pool: Pool<Sqlite>,
}

impl SqliteWebApplicationInstallerRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[async_trait]
impl WebApplicationInstallerRepository for SqliteWebApplicationInstallerRepository {
    async fn upsert_plan(&self, plan: &InstallPlan) -> Result<(), WebApplicationInstallerError> {
        let artifacts = serde_json::to_string(plan.artifacts())
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("artifacts: {e}")))?;
        let db = serde_json::to_string(plan.db())
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("db: {e}")))?;
        let overlays = serde_json::to_string(plan.overlays())
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("overlays: {e}")))?;
        let warnings = serde_json::to_string(plan.warnings())
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("warnings: {e}")))?;
        sqlx::query(
            "INSERT OR REPLACE INTO web_app_plans
             (id, app_id, site_id, artifacts_json, install_path, db_json, overlays_json,
              warnings_json, content_hash, secret_ciphertext, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .bind(plan.id().to_string())
        .bind(plan.app_id())
        .bind(plan.site_id().to_string())
        .bind(artifacts)
        .bind(plan.install_path())
        .bind(db)
        .bind(overlays)
        .bind(warnings)
        .bind(plan.content_hash())
        .bind(plan.secret_ciphertext())
        .bind(plan.created_at().to_rfc3339())
        .bind(plan.expires_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| WebApplicationInstallerError::Persistence(format!("upsert plan: {e}")))?;
        Ok(())
    }

    async fn find_plan(
        &self,
        id: Uuid,
    ) -> Result<Option<InstallPlan>, WebApplicationInstallerError> {
        let row = sqlx::query(
            "SELECT id, app_id, site_id, artifacts_json, install_path, db_json,
                    overlays_json, warnings_json, content_hash, secret_ciphertext,
                    created_at
             FROM web_app_plans WHERE id = ?1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| WebApplicationInstallerError::Persistence(format!("find plan: {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let id: String = row.get("id");
        let app_id: String = row.get("app_id");
        let site_id: String = row.get("site_id");
        let artifacts: String = row.get("artifacts_json");
        let install_path: String = row.get("install_path");
        let db: String = row.get("db_json");
        let overlays: String = row.get("overlays_json");
        let warnings: String = row.get("warnings_json");
        let content_hash: String = row.get("content_hash");
        let secret_ciphertext: String = row.get("secret_ciphertext");
        let created_at: String = row.get("created_at");
        let id = Uuid::parse_str(&id)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("id: {e}")))?;
        let site_id = Uuid::parse_str(&site_id)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("site: {e}")))?;
        let artifacts: Vec<InstallArtifact> = serde_json::from_str(&artifacts)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("artifacts: {e}")))?;
        let db: InstallDb = serde_json::from_str(&db)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("db: {e}")))?;
        let overlays: Vec<InstallOverlay> = serde_json::from_str(&overlays)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("overlays: {e}")))?;
        let warnings: Vec<String> = serde_json::from_str(&warnings)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("warnings: {e}")))?;
        let plan = InstallPlan::new(
            id,
            app_id,
            site_id,
            artifacts,
            install_path,
            db,
            overlays,
            warnings,
            content_hash,
            secret_ciphertext,
            parse_ts(&created_at),
        )
        .map_err(|e| WebApplicationInstallerError::Persistence(format!("plan: {e}")))?;
        Ok(Some(plan))
    }

    async fn insert_run(&self, run: &InstallRun) -> Result<(), WebApplicationInstallerError> {
        sqlx::query(
            "INSERT INTO web_app_runs
             (id, plan_id, site_id, app_id, install_id, install_path, post_install_url,
              started_at, finished_at, failure_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL)",
        )
        .bind(run.id().to_string())
        .bind(run.plan_id().to_string())
        .bind(run.site_id().to_string())
        .bind(run.app_id())
        .bind(run.install_id())
        .bind(run.install_path())
        .bind(run.post_install_url())
        .bind(run.started_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| WebApplicationInstallerError::Persistence(format!("insert run: {e}")))?;
        Ok(())
    }

    async fn finish_run(
        &self,
        run_id: Uuid,
        finished_at: DateTime<Utc>,
        failure_reason: Option<String>,
    ) -> Result<(), WebApplicationInstallerError> {
        sqlx::query("UPDATE web_app_runs SET finished_at = ?1, failure_reason = ?2 WHERE id = ?3")
            .bind(finished_at.to_rfc3339())
            .bind(failure_reason)
            .bind(run_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("finish run: {e}")))?;
        Ok(())
    }

    async fn find_run_by_idempotency_key(
        &self,
        key: &str,
    ) -> Result<Option<InstallRun>, WebApplicationInstallerError> {
        let row = sqlx::query("SELECT run_id FROM web_app_idempotency WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("find idem: {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let run_id: String = row.get("run_id");
        let run_id = Uuid::parse_str(&run_id)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("run: {e}")))?;
        // Re-load the run via a direct query.
        let row = sqlx::query(
            "SELECT id, plan_id, site_id, app_id, install_id, install_path,
                    post_install_url, started_at
             FROM web_app_runs WHERE id = ?1",
        )
        .bind(run_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| WebApplicationInstallerError::Persistence(format!("run reload: {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let id: String = row.get("id");
        let plan_id: String = row.get("plan_id");
        let site_id: String = row.get("site_id");
        let app_id: String = row.get("app_id");
        let install_id: String = row.get("install_id");
        let install_path: String = row.get("install_path");
        let started_at: String = row.get("started_at");
        let id = Uuid::parse_str(&id)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("id: {e}")))?;
        let plan_id = Uuid::parse_str(&plan_id)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("plan: {e}")))?;
        let site_id = Uuid::parse_str(&site_id)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("site: {e}")))?;
        let mut run = InstallRun::new(
            id,
            plan_id,
            site_id,
            app_id,
            install_id,
            install_path,
            parse_ts(&started_at),
        );
        if let Ok(Some(u)) = row.try_get::<Option<String>, _>("post_install_url") {
            run.set_post_install_url(u);
        }
        Ok(Some(run))
    }

    async fn insert_idempotency_key(
        &self,
        key: &IdempotencyKey,
    ) -> Result<(), WebApplicationInstallerError> {
        sqlx::query(
            "INSERT INTO web_app_idempotency (key, run_id, created_at)
             VALUES (?1, ?2, ?3)",
        )
        .bind(key.key())
        .bind(key.run_id().to_string())
        .bind(key.created_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| WebApplicationInstallerError::Persistence(format!("insert idem: {e}")))?;
        Ok(())
    }

    async fn insert_installed(
        &self,
        installed: &InstalledWebApp,
    ) -> Result<(), WebApplicationInstallerError> {
        sqlx::query(
            "INSERT INTO web_app_installs
             (install_id, site_id, app_id, version, install_path, created_at, removed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
        )
        .bind(installed.install_id())
        .bind(installed.site_id().to_string())
        .bind(installed.app_id())
        .bind(installed.version())
        .bind(installed.install_path())
        .bind(installed.created_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| WebApplicationInstallerError::Persistence(format!("insert installed: {e}")))?;
        Ok(())
    }

    async fn mark_installed_removed(
        &self,
        install_id: &str,
        at: DateTime<Utc>,
    ) -> Result<(), WebApplicationInstallerError> {
        sqlx::query("UPDATE web_app_installs SET removed_at = ?1 WHERE install_id = ?2")
            .bind(at.to_rfc3339())
            .bind(install_id)
            .execute(&self.pool)
            .await
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("mark removed: {e}")))?;
        Ok(())
    }

    async fn find_installed(
        &self,
        site_id: Uuid,
        app_id: &str,
    ) -> Result<Option<InstalledWebApp>, WebApplicationInstallerError> {
        let row = sqlx::query(
            "SELECT install_id, site_id, app_id, version, install_path, created_at, removed_at
             FROM web_app_installs
             WHERE site_id = ?1 AND app_id = ?2
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(site_id.to_string())
        .bind(app_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| WebApplicationInstallerError::Persistence(format!("find installed: {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let install_id: String = row.get("install_id");
        let site_id: String = row.get("site_id");
        let app_id: String = row.get("app_id");
        let version: String = row.get("version");
        let install_path: String = row.get("install_path");
        let created_at: String = row.get("created_at");
        let removed_at: Option<String> = row.get("removed_at");
        let site_id = Uuid::parse_str(&site_id)
            .map_err(|e| WebApplicationInstallerError::Persistence(format!("site: {e}")))?;
        let mut installed = InstalledWebApp::new(
            install_id,
            site_id,
            app_id,
            version,
            install_path,
            parse_ts(&created_at),
        );
        if let Some(s) = removed_at {
            installed.mark_removed(parse_ts(&s));
        }
        Ok(Some(installed))
    }

    async fn list_installed(
        &self,
        site_id: Uuid,
    ) -> Result<Vec<InstalledWebApp>, WebApplicationInstallerError> {
        let rows = sqlx::query(
            "SELECT install_id, site_id, app_id, version, install_path, created_at, removed_at
             FROM web_app_installs
             WHERE site_id = ?1
             ORDER BY created_at DESC",
        )
        .bind(site_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| WebApplicationInstallerError::Persistence(format!("list installed: {e}")))?;
        let mut out = Vec::new();
        for row in rows {
            let install_id: String = row.get("install_id");
            let site_id: String = row.get("site_id");
            let app_id: String = row.get("app_id");
            let version: String = row.get("version");
            let install_path: String = row.get("install_path");
            let created_at: String = row.get("created_at");
            let removed_at: Option<String> = row.get("removed_at");
            let site_id = Uuid::parse_str(&site_id)
                .map_err(|e| WebApplicationInstallerError::Persistence(format!("site: {e}")))?;
            let mut installed = InstalledWebApp::new(
                install_id,
                site_id,
                app_id,
                version,
                install_path,
                parse_ts(&created_at),
            );
            if let Some(s) = removed_at {
                installed.mark_removed(parse_ts(&s));
            }
            out.push(installed);
        }
        Ok(out)
    }
}
