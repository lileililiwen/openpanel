//! Feedback bounded context: SQLite repository, submission service, and
//! composition module for the NPS-style admin interaction surface.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::{
    FEEDBACK_RATE_LIMIT_PER_24H, FEEDBACK_RATE_WINDOW_SECS, FeedbackEntry, FeedbackError,
    FeedbackRepository, RepoError, Sentiment, rate_limit_exceeded,
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// Stable module name.
pub const MODULE_NAME: &str = "feedback";

/// SQLite-backed feedback repository.
pub struct SqliteFeedbackRepository {
    pool: Pool<Sqlite>,
}

impl SqliteFeedbackRepository {
    /// Construct over an initialized pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FeedbackRepository for SqliteFeedbackRepository {
    async fn insert(&self, entry: &FeedbackEntry) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO feedback(id,account_id,sentiment,comment,created_at) VALUES(?,?,?,?,?)",
        )
        .bind(entry.id().to_string())
        .bind(entry.account_id().to_string())
        .bind(entry.sentiment().as_str())
        .bind(entry.comment())
        .bind(entry.created_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn count_in_window(
        &self,
        account_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<u64, RepoError> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM feedback WHERE account_id=? AND created_at >= ?",
        )
        .bind(account_id.to_string())
        .bind(since.to_rfc3339())
        .fetch_one(&self.pool)
        .await
        .map_err(repo_error)?;
        let count: i64 = row.try_get("n").map_err(repo_error)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn find_recent(
        &self,
        account_id: Uuid,
        limit: u32,
    ) -> Result<Vec<FeedbackEntry>, RepoError> {
        let rows = sqlx::query(
            "SELECT id,account_id,sentiment,comment,created_at FROM feedback \
             WHERE account_id=? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(account_id.to_string())
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(repo_error)?;
        rows.into_iter().map(row_entry).collect()
    }
}

fn row_entry(row: sqlx::sqlite::SqliteRow) -> Result<FeedbackEntry, RepoError> {
    let id: String = row.try_get("id").map_err(repo_error)?;
    let account_id: String = row.try_get("account_id").map_err(repo_error)?;
    let sentiment: String = row.try_get("sentiment").map_err(repo_error)?;
    let comment: Option<String> = row.try_get("comment").map_err(repo_error)?;
    let created_at: String = row.try_get("created_at").map_err(repo_error)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map_err(repo_error)?
        .with_timezone(&Utc);
    FeedbackEntry::new(
        Uuid::parse_str(&id).map_err(repo_error)?,
        Uuid::parse_str(&account_id).map_err(repo_error)?,
        Sentiment::parse(&sentiment).map_err(|_| RepoError::new("invalid sentiment row"))?,
        comment,
        created_at,
    )
    .map_err(|error| RepoError::new(error.to_string()))
}

fn repo_error(error: impl std::fmt::Display) -> RepoError {
    RepoError::new(error.to_string())
}

/// Submission + query use-cases for feedback.
pub struct FeedbackService {
    repo: Arc<dyn FeedbackRepository>,
    limit_per_window: u64,
    window_secs: i64,
}

impl FeedbackService {
    /// Build a service over a repository with the domain default budget.
    pub fn new(repo: Arc<dyn FeedbackRepository>) -> Self {
        Self {
            repo,
            limit_per_window: FEEDBACK_RATE_LIMIT_PER_24H,
            window_secs: FEEDBACK_RATE_WINDOW_SECS,
        }
    }

    /// Persist one submission, enforcing the rolling per-window budget.
    pub async fn submit(
        &self,
        account_id: Uuid,
        sentiment: Sentiment,
        comment: Option<String>,
    ) -> Result<FeedbackEntry, FeedbackError> {
        let now = Utc::now();
        let since = now - Duration::seconds(self.window_secs);
        let count = self
            .repo
            .count_in_window(account_id, since)
            .await
            .map_err(persistence)?;
        if rate_limit_exceeded(count, self.limit_per_window) {
            return Err(FeedbackError::RateLimited);
        }
        let entry = FeedbackEntry::new(Uuid::new_v4(), account_id, sentiment, comment, now)
            .map_err(|error| match error {
                FeedbackError::InvalidComment(message) => FeedbackError::InvalidComment(message),
                other => other,
            })?;
        self.repo.insert(&entry).await.map_err(persistence)?;
        Ok(entry)
    }

    /// Number of submissions for one account inside the current window.
    pub async fn count_in_window(&self, account_id: Uuid) -> Result<u64, FeedbackError> {
        let since = Utc::now() - Duration::seconds(self.window_secs);
        self.repo
            .count_in_window(account_id, since)
            .await
            .map_err(persistence)
    }

    /// Most recent submissions for one account.
    pub async fn find_recent(
        &self,
        account_id: Uuid,
        limit: u32,
    ) -> Result<Vec<FeedbackEntry>, FeedbackError> {
        self.repo
            .find_recent(account_id, limit)
            .await
            .map_err(persistence)
    }
}

fn persistence(error: RepoError) -> FeedbackError {
    FeedbackError::Persistence(error.into_inner())
}

/// Feedback module registration.
pub struct FeedbackModule {
    service: Arc<FeedbackService>,
    migrations: Vec<Migration>,
}

impl FeedbackModule {
    /// Compose the SQLite-backed module.
    pub async fn new(ctx: &AppContext) -> Self {
        let repo: Arc<dyn FeedbackRepository> =
            Arc::new(SqliteFeedbackRepository::new(ctx.db.pool().await));
        Self {
            service: Arc::new(FeedbackService::new(repo)),
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "feedback widget submissions".into(),
                sql: crate::migrations::FEEDBACK_V001.into(),
            }],
        }
    }

    /// Shared use-case service.
    pub fn service(&self) -> Arc<FeedbackService> {
        self.service.clone()
    }
}

impl Module for FeedbackModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

#[cfg(test)]
mod tests {
    use openpanel_test_support::TestDb;
    use sqlx::SqlitePool;

    use super::*;

    async fn setup() -> (SqlitePool, TestDb) {
        let db = TestDb::new().await;
        sqlx::query(crate::migrations::FEEDBACK_V001)
            .execute(&db.pool())
            .await
            .expect("apply feedback migration");
        (db.pool().clone(), db)
    }

    #[tokio::test]
    async fn service_defaults_match_domain_constants() {
        let repo = SqliteFeedbackRepository::new(
            Pool::connect_lazy("sqlite::memory:").expect("lazy pool"),
        );
        let svc = FeedbackService::new(Arc::new(repo));
        assert_eq!(svc.limit_per_window, FEEDBACK_RATE_LIMIT_PER_24H);
        assert_eq!(svc.window_secs, FEEDBACK_RATE_WINDOW_SECS);
    }

    #[tokio::test]
    async fn submit_persists_and_rate_limits() {
        let (pool, _db) = setup().await;
        let repo = Arc::new(SqliteFeedbackRepository::new(pool));
        let svc = FeedbackService::new(repo);
        let account = Uuid::new_v4();

        for _ in 0..5 {
            svc.submit(account, Sentiment::Up, Some("great".into()))
                .await
                .expect("submission within budget");
        }
        let sixth = svc
            .submit(account, Sentiment::Down, None)
            .await
            .expect_err("sixth submission is rate limited");
        assert_eq!(sixth, FeedbackError::RateLimited);
        assert_eq!(
            svc.count_in_window(account).await.expect("count"),
            5,
            "exactly five rows persisted"
        );
    }

    #[tokio::test]
    async fn find_recent_returns_most_recent_first() {
        let (pool, _db) = setup().await;
        let repo = Arc::new(SqliteFeedbackRepository::new(pool));
        let svc = FeedbackService::new(repo);
        let account = Uuid::new_v4();
        let _ = svc
            .submit(account, Sentiment::Down, None)
            .await
            .expect("one");
        let _ = svc.submit(account, Sentiment::Up, None).await.expect("two");

        let recent = svc.find_recent(account, 2).await.expect("recent");
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].sentiment(), Sentiment::Up, "latest first");
        assert_eq!(recent[1].sentiment(), Sentiment::Down);
        let limited = svc.find_recent(account, 1).await.expect("limited");
        assert_eq!(limited.len(), 1);
    }

    #[tokio::test]
    async fn submit_rejects_oversized_comment() {
        let (pool, _db) = setup().await;
        let repo = Arc::new(SqliteFeedbackRepository::new(pool));
        let svc = FeedbackService::new(repo);
        let account = Uuid::new_v4();
        let result = svc
            .submit(account, Sentiment::Up, Some("x".repeat(2001)))
            .await
            .expect_err("oversized comment rejected");
        assert!(matches!(result, FeedbackError::InvalidComment(_)));
        assert_eq!(svc.count_in_window(account).await.expect("count"), 0);
    }
}
