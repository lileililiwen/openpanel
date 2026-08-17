//! SQLite-backed adapter for the migration-importers bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    DriverKind, ImportedResource, ImportedResourceKind, MigrationError, MigrationRepository,
    MigrationRun, MigrationRunId, MigrationRunStatus, TranslationLogEntry,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed migration repository.
#[derive(Clone)]
pub struct SqliteMigrationRepository {
    pool: Pool<Sqlite>,
}

impl SqliteMigrationRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn driver_str(driver: DriverKind) -> &'static str {
    driver.as_str()
}

fn driver_from_str(s: &str) -> DriverKind {
    match s {
        "cpanel-legacy-backup" => DriverKind::CpanelLegacyBackup,
        "baota-backup" => DriverKind::BaotaBackup,
        "tar-with-json-manifest" => DriverKind::TarWithJsonManifest,
        _ => DriverKind::CpanelPkgacct,
    }
}

fn status_str(status: MigrationRunStatus) -> &'static str {
    match status {
        MigrationRunStatus::InProgress => "in_progress",
        MigrationRunStatus::Completed => "completed",
        MigrationRunStatus::Failed => "failed",
        MigrationRunStatus::RolledBack => "rolled_back",
    }
}

fn status_from_str(s: &str) -> MigrationRunStatus {
    match s {
        "completed" => MigrationRunStatus::Completed,
        "failed" => MigrationRunStatus::Failed,
        "rolled_back" => MigrationRunStatus::RolledBack,
        _ => MigrationRunStatus::InProgress,
    }
}

fn kind_str(kind: ImportedResourceKind) -> &'static str {
    match kind {
        ImportedResourceKind::User => "user",
        ImportedResourceKind::Site => "site",
        ImportedResourceKind::Database => "database",
        ImportedResourceKind::MailDomain => "mail_domain",
        ImportedResourceKind::Mailbox => "mailbox",
        ImportedResourceKind::DnsZone => "dns_zone",
        ImportedResourceKind::CronJob => "cron_job",
        ImportedResourceKind::SslCertificate => "ssl_certificate",
    }
}

fn kind_from_str(s: &str) -> ImportedResourceKind {
    match s {
        "user" => ImportedResourceKind::User,
        "site" => ImportedResourceKind::Site,
        "database" => ImportedResourceKind::Database,
        "mail_domain" => ImportedResourceKind::MailDomain,
        "mailbox" => ImportedResourceKind::Mailbox,
        "dns_zone" => ImportedResourceKind::DnsZone,
        "cron_job" => ImportedResourceKind::CronJob,
        "ssl_certificate" => ImportedResourceKind::SslCertificate,
        _ => ImportedResourceKind::Site,
    }
}

fn outcome_str(outcome: openpanel_domain::TranslationOutcome) -> &'static str {
    match outcome {
        openpanel_domain::TranslationOutcome::Imported => "imported",
        openpanel_domain::TranslationOutcome::Skipped => "skipped",
        openpanel_domain::TranslationOutcome::Failed => "failed",
    }
}

#[allow(dead_code)] // decode counterpart of `outcome_str`; unreachable until log-entry reads exist
fn outcome_from_str(s: &str) -> openpanel_domain::TranslationOutcome {
    match s {
        "imported" => openpanel_domain::TranslationOutcome::Imported,
        "failed" => openpanel_domain::TranslationOutcome::Failed,
        _ => openpanel_domain::TranslationOutcome::Skipped,
    }
}

#[derive(sqlx::FromRow)]
struct RunRow {
    id: String,
    plan_id: String,
    driver: String,
    target_owner_user_id: String,
    confirmed_at: String,
    status: String,
}

impl RunRow {
    fn into_run(self) -> MigrationRun {
        MigrationRun::restore(
            MigrationRunId(Uuid::parse_str(&self.id).unwrap_or_default()),
            openpanel_domain::MigrationPlanId(Uuid::parse_str(&self.plan_id).unwrap_or_default()),
            driver_from_str(&self.driver),
            Uuid::parse_str(&self.target_owner_user_id).unwrap_or_default(),
            parse_ts(&self.confirmed_at),
            status_from_str(&self.status),
        )
    }
}

#[derive(sqlx::FromRow)]
struct ImportedResourceRow {
    run_id: String,
    kind: String,
    source_key: String,
    ref_id: String,
    rolled_back: i64,
}

impl ImportedResourceRow {
    fn into_resource(self) -> ImportedResource {
        ImportedResource::restore(
            MigrationRunId(Uuid::parse_str(&self.run_id).unwrap_or_default()),
            kind_from_str(&self.kind),
            self.source_key,
            Uuid::parse_str(&self.ref_id).unwrap_or_default(),
            self.rolled_back != 0,
        )
    }
}

#[allow(dead_code)] // row mapping reserved for translation_log_entries reads
#[derive(sqlx::FromRow)]
struct LogEntryRow {
    run_id: String,
    kind: String,
    source_key: String,
    outcome: String,
    redacted: String,
}

impl LogEntryRow {
    #[allow(dead_code)] // not yet called; the service writes log entries but never reads them
    fn into_entry(self) -> TranslationLogEntry {
        TranslationLogEntry {
            run_id: MigrationRunId(Uuid::parse_str(&self.run_id).unwrap_or_default()),
            kind: kind_from_str(&self.kind),
            source_key: self.source_key,
            outcome: outcome_from_str(&self.outcome),
            redacted: self.redacted,
        }
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[async_trait]
impl MigrationRepository for SqliteMigrationRepository {
    async fn insert_run(&self, run: &MigrationRun) -> Result<(), MigrationError> {
        sqlx::query("INSERT INTO migration_runs (id, plan_id, driver, target_owner_user_id, confirmed_at, status) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(run.run_id().as_uuid().to_string())
            .bind(run.plan_id().as_uuid().to_string())
            .bind(driver_str(run.driver()))
            .bind(run.target_owner_user_id().to_string())
            .bind(run.confirmed_at().to_rfc3339())
            .bind(status_str(run.status()))
            .execute(&self.pool)
            .await
            .map_err(|e| MigrationError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn update_run_status(&self, run: &MigrationRun) -> Result<(), MigrationError> {
        sqlx::query("UPDATE migration_runs SET status = ? WHERE id = ?")
            .bind(status_str(run.status()))
            .bind(run.run_id().as_uuid().to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| MigrationError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn insert_log_entry(&self, entry: &TranslationLogEntry) -> Result<(), MigrationError> {
        sqlx::query("INSERT INTO translation_log_entries (run_id, kind, source_key, outcome, redacted) VALUES (?, ?, ?, ?, ?)")
            .bind(entry.run_id.as_uuid().to_string())
            .bind(kind_str(entry.kind))
            .bind(&entry.source_key)
            .bind(outcome_str(entry.outcome))
            .bind(&entry.redacted)
            .execute(&self.pool)
            .await
            .map_err(|e| MigrationError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn insert_imported_resource(
        &self,
        resource: &ImportedResource,
    ) -> Result<(), MigrationError> {
        sqlx::query("INSERT INTO imported_resources (run_id, kind, source_key, ref_id, rolled_back) VALUES (?, ?, ?, ?, ?)")
            .bind(resource.run_id.as_uuid().to_string())
            .bind(kind_str(resource.kind))
            .bind(&resource.source_key)
            .bind(resource.ref_id().to_string())
            .bind(i64::from(resource.rolled_back))
            .execute(&self.pool)
            .await
            .map_err(|e| MigrationError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn mark_imported_resource_rolled_back(
        &self,
        resource: &ImportedResource,
    ) -> Result<(), MigrationError> {
        sqlx::query(
            "UPDATE imported_resources SET rolled_back = 1 WHERE run_id = ? AND ref_id = ?",
        )
        .bind(resource.run_id.as_uuid().to_string())
        .bind(resource.ref_id().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| MigrationError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn recent_runs(&self, limit: i64) -> Result<Vec<MigrationRun>, MigrationError> {
        let rows = sqlx::query_as::<_, RunRow>(
            "SELECT id, plan_id, driver, target_owner_user_id, confirmed_at, status FROM migration_runs ORDER BY confirmed_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MigrationError::Persistence(e.to_string()))?;
        Ok(rows.into_iter().map(RunRow::into_run).collect())
    }

    async fn imported_resources(
        &self,
        run_id: MigrationRunId,
    ) -> Result<Vec<ImportedResource>, MigrationError> {
        let rows = sqlx::query_as::<_, ImportedResourceRow>(
            "SELECT run_id, kind, source_key, ref_id, rolled_back FROM imported_resources WHERE run_id = ? ORDER BY id DESC",
        )
        .bind(run_id.as_uuid().to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MigrationError::Persistence(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(ImportedResourceRow::into_resource)
            .collect())
    }
}
