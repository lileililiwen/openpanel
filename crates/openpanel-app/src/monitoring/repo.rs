//! SQLite-backed adapter for `SnapshotRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    RepoError,
    monitoring::{
        metric::{MetricKind, MetricSample, Unit},
        repository::SnapshotRepository,
    },
};
use sqlx::{Pool, Row, Sqlite};

/// SQLite-backed adapter for the domain `SnapshotRepository` trait.
#[derive(Clone)]
pub struct SqliteSnapshotRepository {
    pool: Pool<Sqlite>,
}

impl SqliteSnapshotRepository {
    /// Build a repository over the given SQLite connection pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn kind_as_str(kind: MetricKind) -> &'static str {
    kind.as_str()
}

fn parse_kind(s: &str) -> MetricKind {
    s.parse().unwrap_or(MetricKind::Cpu)
}

fn parse_unit(kind: MetricKind) -> Unit {
    match kind {
        MetricKind::Cpu | MetricKind::Memory | MetricKind::Disk => Unit::Percent,
        MetricKind::Network => Unit::BytesPerSecond,
    }
}

#[async_trait]
impl SnapshotRepository for SqliteSnapshotRepository {
    async fn insert(&self, sample: &MetricSample) -> Result<(), RepoError> {
        sqlx::query("INSERT INTO monitoring_samples (ts, kind, value) VALUES (?, ?, ?)")
            .bind(sample.ts.to_rfc3339())
            .bind(kind_as_str(sample.kind))
            .bind(sample.value)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("UNIQUE") {
                    RepoError::new("unique violation: sample already exists")
                } else {
                    RepoError::new(msg)
                }
            })?;
        Ok(())
    }

    async fn latest(&self) -> Result<Vec<MetricSample>, RepoError> {
        let rows = sqlx::query(
            "SELECT ts, kind, value FROM monitoring_samples
             WHERE (kind, ts) IN (
                 SELECT kind, MAX(ts) FROM monitoring_samples GROUP BY kind
             ) ORDER BY kind",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_sample).collect()
    }

    async fn history(
        &self,
        kind: MetricKind,
        since: DateTime<Utc>,
    ) -> Result<Vec<MetricSample>, RepoError> {
        let rows = sqlx::query(
            "SELECT ts, kind, value FROM monitoring_samples
             WHERE kind = ? AND ts >= ? ORDER BY ts ASC",
        )
        .bind(kind_as_str(kind))
        .bind(since.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_sample).collect()
    }

    async fn prune(&self, before: DateTime<Utc>) -> Result<u64, RepoError> {
        let res = sqlx::query("DELETE FROM monitoring_samples WHERE ts < ?")
            .bind(before.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(res.rows_affected())
    }
}

fn row_to_sample(row: sqlx::sqlite::SqliteRow) -> Result<MetricSample, RepoError> {
    let ts: String = row
        .try_get("ts")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let ts = DateTime::parse_from_rfc3339(&ts)
        .map_err(|e| RepoError::new(e.to_string()))?
        .with_timezone(&Utc);
    let kind_str: String = row
        .try_get("kind")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let kind = parse_kind(&kind_str);
    let value: f64 = row
        .try_get("value")
        .map_err(|e| RepoError::new(e.to_string()))?;
    Ok(MetricSample {
        kind,
        unit: parse_unit(kind),
        value,
        ts,
    })
}

#[cfg(test)]
mod tests {
    //! Tests against a real SQLite database via `TestDb`. These cover
    //! the round-trip the domain fake cannot: actual SQL, the `UNIQUE`
    //! constraint, `ORDER BY`, and `DELETE`.

    use chrono::{Duration, Utc};
    use openpanel_domain::{
        SnapshotRepository,
        monitoring::metric::{MetricKind, MetricSample, Unit},
    };
    use openpanel_test_support::TestDb;

    use super::SqliteSnapshotRepository;

    fn sample(ts: chrono::DateTime<Utc>, kind: MetricKind, value: f64) -> MetricSample {
        MetricSample {
            kind,
            unit: match kind {
                MetricKind::Cpu | MetricKind::Memory | MetricKind::Disk => Unit::Percent,
                MetricKind::Network => Unit::BytesPerSecond,
            },
            value,
            ts,
        }
    }
    #[tokio::test]
    async fn insert_latest_history_round_trip() {
        let db = TestDb::new().await;
        sqlx::query(include_str!("../migrations/monitoring/V001__init.sql"))
            .execute(&db.pool())
            .await
            .expect("monitoring migration");
        let repo = SqliteSnapshotRepository::new(db.pool());

        let now = Utc::now();
        let cpu1 = sample(now, MetricKind::Cpu, 10.0);
        let cpu2 = sample(now + Duration::seconds(60), MetricKind::Cpu, 20.0);
        let mem = sample(now, MetricKind::Memory, 40.0);
        repo.insert(&cpu1).await.unwrap();
        repo.insert(&cpu2).await.unwrap();
        repo.insert(&mem).await.unwrap();

        let latest = repo.latest().await.unwrap();
        assert_eq!(latest.len(), 2, "one row per kind");
        let cpu_latest = latest
            .iter()
            .find(|s| s.kind == MetricKind::Cpu)
            .expect("cpu present");
        assert_eq!(cpu_latest.value, 20.0, "latest cpu is the newest");

        let history = repo
            .history(MetricKind::Cpu, now - Duration::seconds(10))
            .await
            .unwrap();
        assert_eq!(history.len(), 2, "both cpu samples returned");
        assert_eq!(history[0].value, 10.0, "ascending order");
        assert_eq!(history[1].value, 20.0);
    }

    #[tokio::test]
    async fn duplicate_insert_returns_unique_violation() {
        let db = TestDb::new().await;
        sqlx::query(include_str!("../migrations/monitoring/V001__init.sql"))
            .execute(&db.pool())
            .await
            .expect("monitoring migration");
        let repo = SqliteSnapshotRepository::new(db.pool());

        let now = Utc::now();
        let s = sample(now, MetricKind::Cpu, 1.0);
        repo.insert(&s).await.unwrap();
        let err = repo.insert(&s).await;
        match err {
            Err(e) => assert!(
                e.0.contains("unique violation"),
                "second insert of same (ts, kind) must violate UNIQUE, got {e:?}"
            ),
            Ok(()) => panic!("second insert of same (ts, kind) must fail"),
        }
    }

    #[tokio::test]
    async fn history_filters_by_kind_and_orders_ascending() {
        let db = TestDb::new().await;
        sqlx::query(include_str!("../migrations/monitoring/V001__init.sql"))
            .execute(&db.pool())
            .await
            .expect("monitoring migration");
        let repo = SqliteSnapshotRepository::new(db.pool());

        let now = Utc::now();
        for (i, kind) in [MetricKind::Cpu, MetricKind::Memory, MetricKind::Disk]
            .into_iter()
            .enumerate()
        {
            repo.insert(&sample(now + Duration::seconds(i as i64), kind, i as f64))
                .await
                .unwrap();
        }
        let disk = repo.history(MetricKind::Disk, now).await.unwrap();
        assert_eq!(disk.len(), 1);
        assert_eq!(disk[0].kind, MetricKind::Disk);
        assert_eq!(disk[0].value, 2.0);
    }

    #[tokio::test]
    async fn prune_deletes_only_old_rows() {
        let db = TestDb::new().await;
        sqlx::query(include_str!("../migrations/monitoring/V001__init.sql"))
            .execute(&db.pool())
            .await
            .expect("monitoring migration");
        let repo = SqliteSnapshotRepository::new(db.pool());

        let now = Utc::now();
        let old = sample(now - Duration::days(2), MetricKind::Cpu, 1.0);
        let recent = sample(now, MetricKind::Cpu, 2.0);
        repo.insert(&old).await.unwrap();
        repo.insert(&recent).await.unwrap();

        let deleted = repo.prune(now - Duration::days(1)).await.unwrap();
        assert_eq!(deleted, 1, "only the old row is deleted");

        let remaining = repo
            .history(MetricKind::Cpu, now - Duration::days(3))
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].value, 2.0);
    }
}
