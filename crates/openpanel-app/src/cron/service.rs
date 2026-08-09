//! Cron use cases and SQLite persistence.

use std::{path::PathBuf, time::Duration};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::cron::{
    CronJob, CronSchedule, JobKind, JobRun, OverlapPolicy, OwnedWorkingDirectory,
};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use thiserror::Error;
use uuid::Uuid;

/// Input shared by API and CLI job creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronInput {
    /// Display name.
    pub name: String,
    /// Five-field expression.
    pub schedule: String,
    /// IANA timezone.
    pub timezone: String,
    /// `command` or `http`.
    pub kind: String,
    /// Absolute command executable.
    pub executable: Option<String>,
    /// Direct process arguments.
    #[serde(default)]
    pub arguments: Vec<String>,
    /// Command working directory.
    pub working_directory: Option<String>,
    /// HTTP target URL.
    pub url: Option<String>,
    /// HTTP method.
    pub method: Option<String>,
    /// Hard execution timeout.
    pub timeout_secs: u64,
    /// Must currently be `skip`.
    pub overlap_policy: String,
}

/// Mutable scheduling fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronUpdate {
    /// New name when present.
    pub name: Option<String>,
    /// New expression when present.
    pub schedule: Option<String>,
    /// New timezone when present.
    pub timezone: Option<String>,
    /// New timeout when present.
    pub timeout_secs: Option<u64>,
    /// New overlap policy when present.
    pub overlap_policy: Option<String>,
}

/// Typed application failures.
#[derive(Debug, Error)]
pub enum CronServiceError {
    /// Invalid request data.
    #[error("validation failed: {0}")]
    Validation(String),
    /// Requested record is absent or hidden by ownership.
    #[error("cron record not found")]
    NotFound,
    /// Persistent storage failed.
    #[error("cron persistence failed: {0}")]
    Persistence(String),
}

/// Persistent cron job service.
#[derive(Clone)]
pub struct CronService {
    pool: Pool<Sqlite>,
    allowed_roots: Vec<PathBuf>,
    audit: std::sync::Arc<dyn AuditService>,
}

impl CronService {
    /// Construct over a SQLite pool and a set of owned path roots.
    pub fn new(
        pool: Pool<Sqlite>,
        allowed_roots: Vec<PathBuf>,
        audit: std::sync::Arc<dyn AuditService>,
    ) -> Self {
        Self {
            pool,
            allowed_roots,
            audit,
        }
    }

    /// Create a job owned by `owner_id`.
    pub async fn create(
        &self,
        owner_id: Uuid,
        input: CronInput,
    ) -> Result<CronJob, CronServiceError> {
        let job = self.build(owner_id, input)?;
        let payload = job.to_json().map_err(validation)?;
        sqlx::query("INSERT INTO cron_jobs (id, owner_id, payload) VALUES (?, ?, ?)")
            .bind(job.id().to_string())
            .bind(owner_id.to_string())
            .bind(payload)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        self.audit_change(owner_id, job.id(), "created").await;
        Ok(job)
    }

    fn build(&self, owner_id: Uuid, input: CronInput) -> Result<CronJob, CronServiceError> {
        if input.overlap_policy != "skip" {
            return Err(CronServiceError::Validation(
                "overlap_policy must be skip".into(),
            ));
        }
        let schedule = CronSchedule::parse(input.schedule, input.timezone).map_err(validation)?;
        let kind = match input.kind.as_str() {
            "command" => {
                let directory = input.working_directory.ok_or_else(|| {
                    CronServiceError::Validation("working_directory is required".into())
                })?;
                let directory = OwnedWorkingDirectory::new(directory, &self.allowed_roots)
                    .map_err(validation)?;
                JobKind::command(
                    input.executable.unwrap_or_default(),
                    input.arguments,
                    directory,
                )
                .map_err(validation)?
            }
            "http" => JobKind::http(
                input.url.unwrap_or_default(),
                input.method.unwrap_or_else(|| "GET".into()),
            )
            .map_err(validation)?,
            _ => {
                return Err(CronServiceError::Validation(
                    "kind must be command or http".into(),
                ));
            }
        };
        CronJob::new(
            Uuid::new_v4(),
            owner_id,
            input.name,
            schedule,
            kind,
            Duration::from_secs(input.timeout_secs),
            OverlapPolicy::Skip,
            Utc::now(),
        )
        .map_err(validation)
    }

    /// List visible jobs. Owners may request all jobs.
    pub async fn list(&self, owner_id: Uuid, all: bool) -> Result<Vec<CronJob>, CronServiceError> {
        let rows = if all {
            sqlx::query("SELECT payload FROM cron_jobs ORDER BY id")
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query("SELECT payload FROM cron_jobs WHERE owner_id = ? ORDER BY id")
                .bind(owner_id.to_string())
                .fetch_all(&self.pool)
                .await
        }
        .map_err(db)?;
        rows.into_iter()
            .map(|row| CronJob::restore(row.get::<String, _>(0).as_str()).map_err(validation))
            .collect()
    }

    /// Fetch one visible job.
    pub async fn get(
        &self,
        owner_id: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<CronJob, CronServiceError> {
        let row = if all {
            sqlx::query("SELECT payload FROM cron_jobs WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
        } else {
            sqlx::query("SELECT payload FROM cron_jobs WHERE id = ? AND owner_id = ?")
                .bind(id.to_string())
                .bind(owner_id.to_string())
                .fetch_optional(&self.pool)
                .await
        }
        .map_err(db)?
        .ok_or(CronServiceError::NotFound)?;
        CronJob::restore(row.get::<String, _>(0).as_str()).map_err(validation)
    }

    /// Update schedule metadata while retaining the work definition.
    pub async fn update(
        &self,
        owner_id: Uuid,
        all: bool,
        id: Uuid,
        update: CronUpdate,
    ) -> Result<CronJob, CronServiceError> {
        let mut job = self.get(owner_id, all, id).await?;
        let schedule = CronSchedule::parse(
            update
                .schedule
                .unwrap_or_else(|| job.schedule().expression().into()),
            update
                .timezone
                .unwrap_or_else(|| job.schedule().timezone().into()),
        )
        .map_err(validation)?;
        let name = update.name.unwrap_or_else(|| job.name().into());
        let timeout = Duration::from_secs(update.timeout_secs.unwrap_or(job.timeout().as_secs()));
        job.update(name, schedule, job.kind().clone(), timeout, Utc::now())
            .map_err(validation)?;
        self.save(&job).await?;
        self.audit_change(owner_id, id, "updated").await;
        Ok(job)
    }

    /// Enable or disable a job.
    pub async fn set_enabled(
        &self,
        owner_id: Uuid,
        all: bool,
        id: Uuid,
        enabled: bool,
    ) -> Result<CronJob, CronServiceError> {
        let mut job = self.get(owner_id, all, id).await?;
        if enabled {
            job.enable(Utc::now()).map_err(validation)?;
        } else {
            job.disable();
        }
        self.save(&job).await?;
        self.audit_change(owner_id, id, if enabled { "enabled" } else { "disabled" })
            .await;
        Ok(job)
    }

    async fn save(&self, job: &CronJob) -> Result<(), CronServiceError> {
        sqlx::query("UPDATE cron_jobs SET payload = ? WHERE id = ?")
            .bind(job.to_json().map_err(validation)?)
            .bind(job.id().to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// Delete a visible job and its run history.
    pub async fn delete(
        &self,
        owner_id: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<(), CronServiceError> {
        self.get(owner_id, all, id).await?;
        sqlx::query("DELETE FROM cron_jobs WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        self.audit_change(owner_id, id, "deleted").await;
        Ok(())
    }

    /// Create durable history then execute a command without a shell.
    pub async fn run_now(
        &self,
        owner_id: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<JobRun, CronServiceError> {
        let job = self.get(owner_id, all, id).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    owner_id.to_string(),
                    AuditAction::CronRun,
                    AuditOutcome::Success,
                )
                .target(id.to_string()),
            )
            .await;
        let now = Utc::now();
        let active = sqlx::query("SELECT payload FROM cron_runs WHERE job_id = ?")
            .bind(id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .into_iter()
            .filter_map(|row| serde_json::from_str::<JobRun>(row.get::<String, _>(0).as_str()).ok())
            .any(|run| !run.is_terminal());
        if active {
            let run = JobRun::skipped(Uuid::new_v4(), id, now);
            self.insert_run(job.owner_id(), &run, now).await?;
            return Ok(run);
        }
        let mut run = JobRun::leased(Uuid::new_v4(), id, now, now + chrono::Duration::seconds(30));
        run.start(now).map_err(validation)?;
        if let Err(CronServiceError::Persistence(message)) =
            self.insert_run(job.owner_id(), &run, now).await
        {
            if message.contains("UNIQUE constraint failed") {
                let skipped = JobRun::skipped(Uuid::new_v4(), id, now);
                self.insert_run(job.owner_id(), &skipped, now).await?;
                return Ok(skipped);
            }
            return Err(CronServiceError::Persistence(message));
        }
        match job.kind() {
            JobKind::Command {
                executable,
                arguments,
                working_directory,
            } => {
                let child = tokio::process::Command::new(executable)
                    .args(arguments)
                    .current_dir(working_directory.as_path())
                    .env_clear()
                    .kill_on_drop(true)
                    .output();
                match tokio::time::timeout(job.timeout(), child).await {
                    Ok(Ok(output)) => {
                        let code = output.status.code().unwrap_or(-1);
                        run.complete(Utc::now(), code, cap(output.stdout), cap(output.stderr))
                            .map_err(validation)?;
                    }
                    Ok(Err(error)) => {
                        run.complete(Utc::now(), -1, vec![], cap(error.to_string().into_bytes()))
                            .map_err(validation)?;
                    }
                    Err(_) => run.timeout(Utc::now()).map_err(validation)?,
                }
            }
            JobKind::Http { url, method } => {
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .build()
                    .map_err(|e| CronServiceError::Validation(e.to_string()))?;
                let method = reqwest::Method::from_bytes(method.as_bytes())
                    .map_err(|e| CronServiceError::Validation(e.to_string()))?;
                match tokio::time::timeout(job.timeout(), client.request(method, url).send()).await
                {
                    Ok(Ok(response)) => {
                        let status = response.status();
                        let code = i32::from(status.as_u16());
                        let body = response
                            .bytes()
                            .await
                            .map(|bytes| cap(bytes.to_vec()))
                            .unwrap_or_default();
                        run.complete(
                            Utc::now(),
                            if status.is_success() { 0 } else { code },
                            body,
                            vec![],
                        )
                        .map_err(validation)?;
                    }
                    Ok(Err(error)) => run
                        .complete(Utc::now(), -1, vec![], cap(error.to_string().into_bytes()))
                        .map_err(validation)?,
                    Err(_) => run.timeout(Utc::now()).map_err(validation)?,
                }
            }
        }
        self.update_run(&run).await?;
        Ok(run)
    }

    async fn insert_run(
        &self,
        owner_id: Uuid,
        run: &JobRun,
        now: chrono::DateTime<Utc>,
    ) -> Result<(), CronServiceError> {
        let payload =
            serde_json::to_string(run).map_err(|e| CronServiceError::Persistence(e.to_string()))?;
        sqlx::query("INSERT INTO cron_runs (id, job_id, owner_id, payload, active, created_at) VALUES (?, ?, ?, ?, ?, ?)").bind(run.id().to_string()).bind(run.job_id().to_string()).bind(owner_id.to_string()).bind(payload).bind(!run.is_terminal()).bind(now.to_rfc3339()).execute(&self.pool).await.map_err(db)?;
        Ok(())
    }

    async fn update_run(&self, run: &JobRun) -> Result<(), CronServiceError> {
        sqlx::query("UPDATE cron_runs SET payload = ?, active = ? WHERE id = ?")
            .bind(
                serde_json::to_string(run)
                    .map_err(|e| CronServiceError::Persistence(e.to_string()))?,
            )
            .bind(!run.is_terminal())
            .bind(run.id().to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// Execute all enabled jobs due at `now`, advance schedules, recover expired leases, and prune retention.
    pub async fn tick(
        &self,
        now: chrono::DateTime<Utc>,
        retention_days: i64,
    ) -> Result<usize, CronServiceError> {
        self.recover_expired(now).await?;
        let due: Vec<CronJob> = self
            .list(Uuid::nil(), true)
            .await?
            .into_iter()
            .filter(|job| job.enabled() && job.next_run_at() <= now)
            .collect();
        let count = due.len();
        for mut job in due {
            job.advance(now).map_err(validation)?;
            self.save(&job).await?;
            let _ = self.run_now(job.owner_id(), true, job.id()).await?;
        }
        if retention_days > 0 {
            let cutoff = now - chrono::Duration::days(retention_days);
            let rows = sqlx::query("SELECT id, payload FROM cron_runs WHERE created_at < ?")
                .bind(cutoff.to_rfc3339())
                .fetch_all(&self.pool)
                .await
                .map_err(db)?;
            for row in rows {
                let run: JobRun = serde_json::from_str(row.get::<String, _>(1).as_str())
                    .map_err(|e| CronServiceError::Persistence(e.to_string()))?;
                if run.is_terminal() {
                    sqlx::query("DELETE FROM cron_runs WHERE id = ?")
                        .bind(row.get::<String, _>(0))
                        .execute(&self.pool)
                        .await
                        .map_err(db)?;
                }
            }
        }
        Ok(count)
    }

    async fn recover_expired(&self, now: chrono::DateTime<Utc>) -> Result<(), CronServiceError> {
        let rows = sqlx::query("SELECT payload FROM cron_runs")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        for row in rows {
            let mut run: JobRun = serde_json::from_str(row.get::<String, _>(0).as_str())
                .map_err(|e| CronServiceError::Persistence(e.to_string()))?;
            if !run.is_terminal() && run.lease_until() < now {
                run.interrupt(now).map_err(validation)?;
                self.update_run(&run).await?;
            }
        }
        Ok(())
    }

    /// List visible run history, optionally for one job.
    pub async fn runs(
        &self,
        owner_id: Uuid,
        all: bool,
        job_id: Option<Uuid>,
    ) -> Result<Vec<JobRun>, CronServiceError> {
        let jobs = self.list(owner_id, all).await?;
        let visible: Vec<String> = jobs.iter().map(|j| j.id().to_string()).collect();
        let rows = sqlx::query("SELECT job_id, payload FROM cron_runs ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.into_iter()
            .filter(|row| {
                let stored: String = row.get(0);
                visible.contains(&stored) && job_id.is_none_or(|id| stored == id.to_string())
            })
            .map(|row| {
                serde_json::from_str::<JobRun>(row.get::<String, _>(1).as_str())
                    .map_err(|e| CronServiceError::Persistence(e.to_string()))
            })
            .collect()
    }

    /// Fetch one visible run.
    pub async fn get_run(
        &self,
        owner_id: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<JobRun, CronServiceError> {
        self.runs(owner_id, all, None)
            .await?
            .into_iter()
            .find(|run| run.id() == id)
            .ok_or(CronServiceError::NotFound)
    }

    async fn audit_change(&self, actor: Uuid, target: Uuid, operation: &str) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::CronChanged,
                    AuditOutcome::Success,
                )
                .target(target.to_string())
                .metadata(serde_json::json!({"operation": operation})),
            )
            .await;
    }
}

fn cap(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.truncate(64 * 1024);
    bytes
}
fn validation(error: impl std::fmt::Display) -> CronServiceError {
    CronServiceError::Validation(error.to_string())
}
fn db(error: sqlx::Error) -> CronServiceError {
    CronServiceError::Persistence(error.to_string())
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    #[tokio::test]
    async fn persists_jobs_and_rejects_paths_outside_owned_roots() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(crate::migrations::CRON_V001)
            .execute(&pool)
            .await
            .unwrap();
        let svc = CronService::new(
            pool,
            vec![PathBuf::from("/owned")],
            std::sync::Arc::new(openpanel_test_support::MockAudit::stub()),
        );
        let input = |path: &str| CronInput {
            name: "echo".into(),
            schedule: "*/5 * * * *".into(),
            timezone: "UTC".into(),
            kind: "command".into(),
            executable: Some("/bin/echo".into()),
            arguments: vec!["hello".into()],
            working_directory: Some(path.into()),
            url: None,
            method: None,
            timeout_secs: 60,
            overlap_policy: "skip".into(),
        };
        assert!(matches!(
            svc.create(Uuid::new_v4(), input("/outside")).await,
            Err(CronServiceError::Validation(_))
        ));
        let owner = Uuid::new_v4();
        let job = svc.create(owner, input("/owned/site")).await.unwrap();
        assert_eq!(
            svc.get(owner, false, job.id()).await.unwrap().name(),
            "echo"
        );
        assert!(matches!(
            svc.get(Uuid::new_v4(), false, job.id()).await,
            Err(CronServiceError::NotFound)
        ));
    }

    #[tokio::test]
    async fn overlapping_execution_is_skipped_and_timeout_is_terminal() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(crate::migrations::CRON_V001)
            .execute(&pool)
            .await
            .unwrap();
        let svc = CronService::new(
            pool,
            vec![PathBuf::from("/tmp")],
            std::sync::Arc::new(openpanel_test_support::MockAudit::stub()),
        );
        let owner = Uuid::new_v4();
        let job = svc
            .create(
                owner,
                CronInput {
                    name: "sleep".into(),
                    schedule: "*/5 * * * *".into(),
                    timezone: "UTC".into(),
                    kind: "command".into(),
                    executable: Some("/bin/sleep".into()),
                    arguments: vec!["1".into()],
                    working_directory: Some("/tmp".into()),
                    url: None,
                    method: None,
                    timeout_secs: 1,
                    overlap_policy: "skip".into(),
                },
            )
            .await
            .unwrap();
        let first_svc = svc.clone();
        let first_id = job.id();
        let first =
            tokio::spawn(async move { first_svc.run_now(owner, false, first_id).await.unwrap() });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let second = svc.run_now(owner, false, job.id()).await.unwrap();
        assert_eq!(second.state(), openpanel_domain::cron::RunState::Skipped);
        assert!(first.await.unwrap().is_terminal());
    }
}
