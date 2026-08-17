//! Auto-generated test module (consolidated from inline `#[cfg(test)] mod` blocks)

use std::sync::Arc;

use openpanel_core::AuditAction;
use openpanel_domain::{
    BinlogRange, BinlogSink, BinlogStreamRepository, BinlogStreamStatus, IncrementalMode,
    IncrementalRepository, LogSeq, LogTailer, PitrError, PitrRepository, PitrRestoreStatus, Role,
    StreamTargetId,
};
use uuid::Uuid;

use super::*;

#[cfg(test)]
mod tests_2 {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use chrono::Utc;
    use openpanel_core::AuditService;
    use openpanel_domain::databases::{engine::DatabaseEngine, status::DatabaseStatus};
    use openpanel_test_support::mocks::{MockAudit, MockDatabaseRepo};
    use tokio::sync::Mutex;

    use super::*;

    /// `(start_seq, end_seq, started_at, last_applied_at)` for a
    /// streamed range.
    type SegmentRange = (LogSeq, LogSeq, chrono::DateTime<Utc>, chrono::DateTime<Utc>);

    #[derive(Default)]
    struct MemorySink {
        segments: Mutex<HashMap<(Uuid, String), openpanel_domain::BinlogSegment>>,
        ranges: Mutex<HashMap<Uuid, SegmentRange>>,
    }

    #[async_trait]
    impl openpanel_domain::BinlogSink for MemorySink {
        async fn append(
            &self,
            database_id: Uuid,
            segment: &openpanel_domain::BinlogSegment,
        ) -> Result<LogSeq, PitrError> {
            let mut g = self.segments.lock().await;
            g.insert((database_id, segment.from.to_hex()), segment.clone());
            let mut r = self.ranges.lock().await;
            let entry = r.entry(database_id).or_insert_with(|| {
                (
                    segment.from.clone(),
                    segment.to.clone(),
                    segment.generated_at,
                    segment.generated_at,
                )
            });
            entry.1 = segment.to.clone();
            if segment.generated_at < entry.2 {
                entry.2 = segment.generated_at;
            }
            if segment.generated_at > entry.3 {
                entry.3 = segment.generated_at;
            }
            Ok(segment.to.clone())
        }

        async fn replay_window(
            &self,
            database_id: Uuid,
            _earliest: LogSeq,
            _target_ts: chrono::DateTime<Utc>,
        ) -> Result<Vec<openpanel_domain::BinlogSegment>, PitrError> {
            let g = self.segments.lock().await;
            let mut out: Vec<openpanel_domain::BinlogSegment> = g
                .iter()
                .filter(|((db, _), _)| *db == database_id)
                .map(|(_, seg)| seg.clone())
                .collect();
            out.sort_by(|a, b| a.from.as_bytes().cmp(b.from.as_bytes()));
            Ok(out)
        }

        async fn available_range(&self, database_id: Uuid) -> Result<BinlogRange, PitrError> {
            let r = self.ranges.lock().await;
            if let Some((earliest, latest, start, end)) = r.get(&database_id).cloned() {
                BinlogRange::new(earliest, latest, start, end)
            } else {
                let now = Utc::now();
                Ok(BinlogRange::new(
                    LogSeq::initial(),
                    LogSeq::initial(),
                    now,
                    now,
                )?)
            }
        }
    }

    #[derive(Default)]
    struct MemoryTailer;

    #[async_trait]
    impl openpanel_domain::LogTailer for MemoryTailer {
        async fn next_segment(
            &self,
            _database_id: Uuid,
            _from: LogSeq,
        ) -> Result<Option<openpanel_domain::BinlogSegment>, PitrError> {
            Ok(None)
        }

        async fn enable_streaming(&self, _database_id: Uuid) -> Result<(), PitrError> {
            Ok(())
        }

        async fn disable_streaming(&self, _database_id: Uuid) -> Result<(), PitrError> {
            Ok(())
        }
    }

    use openpanel_test_support::db::TestDb;

    fn seed_database_repo(
        database_id: Uuid,
        owner_id: Uuid,
    ) -> Arc<dyn openpanel_domain::DatabaseLookup> {
        let mut repo = MockDatabaseRepo::new();
        repo.expect_find_by_id().returning(move |id| {
            Ok(Some(openpanel_domain::Database::restore(
                id,
                owner_id,
                "owner_app".to_string(),
                "owner_app".to_string(),
                "localhost".to_string(),
                DatabaseEngine::Mysql,
                "utf8mb4".to_string(),
                DatabaseStatus::Active,
                Utc::now(),
                "owner".to_string(),
            )))
        });
        let _ = database_id;
        Arc::new(openpanel_test_support::mocks::DatabaseRepoAsLookup(repo))
    }

    async fn seed_real_database_row(
        pool: sqlx::Pool<sqlx::Sqlite>,
        database_id: Uuid,
        owner_id: Uuid,
    ) {
        let now = Utc::now();
        // Insert a matching users row so the FK on `databases.owner_id` is satisfied.
        sqlx::query(
            "INSERT OR REPLACE INTO users
                (id, username, email, password_hash, role, created_at, disabled_at, last_login_at)
             VALUES (?, ?, ?, ?, ?, ?, NULL, NULL)",
        )
        .bind(owner_id.to_string())
        .bind("owner")
        .bind("owner@example.com")
        .bind("hash")
        .bind("owner")
        .bind(now.to_rfc3339())
        .execute(&pool)
        .await
        .expect("seed users row");
        sqlx::query(
            "INSERT OR REPLACE INTO databases
                (id, owner_id, name, db_user, db_host, engine, charset,
                 status, password_ciphertext, created_at, created_by)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(database_id.to_string())
        .bind(owner_id.to_string())
        .bind("owner_app")
        .bind("owner_app")
        .bind("localhost")
        .bind("mysql")
        .bind("utf8mb4")
        .bind("active")
        .bind("00")
        .bind(now.to_rfc3339())
        .bind("owner")
        .execute(&pool)
        .await
        .expect("seed databases row");
    }

    async fn build_test_service() -> (PitrService, Uuid, Uuid) {
        let db = TestDb::new().await;
        let pool = db.pool();
        let streams_repo: Arc<dyn BinlogStreamRepository> = Arc::new(
            crate::db_pitr::repo::SqliteBinlogStreamRepository::new(pool.clone()),
        );
        let restores_repo: Arc<dyn PitrRepository> = Arc::new(
            crate::db_pitr::repo::SqlitePitrRepository::new(pool.clone()),
        );
        let inc_repo: Arc<dyn IncrementalRepository> = Arc::new(
            crate::db_pitr::repo::SqliteIncrementalRepository::new(pool.clone()),
        );
        let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
        let owner_id = Uuid::new_v4();
        let database_id = Uuid::new_v4();
        seed_real_database_row(pool.clone(), database_id, owner_id).await;
        let database_repo = seed_database_repo(database_id, owner_id);
        let sink: Arc<dyn BinlogSink> = Arc::new(MemorySink::default());
        let tailer: Arc<dyn LogTailer> = Arc::new(MemoryTailer);
        let service = PitrService::new(
            streams_repo,
            restores_repo,
            inc_repo,
            database_repo,
            sink,
            vec![tailer],
            audit,
        );
        (service, owner_id, database_id)
    }

    fn owner(id: Uuid) -> openpanel_domain::User {
        openpanel_domain::User::new(
            id,
            openpanel_domain::Username::new("owner").unwrap(),
            openpanel_domain::Email::new("owner@example.com").unwrap(),
            openpanel_domain::Password::hash("correct horse battery staple").unwrap(),
            Role::Owner,
        )
    }

    fn reseller(id: Uuid) -> openpanel_domain::User {
        openpanel_domain::User::new(
            id,
            openpanel_domain::Username::new("admin_user").unwrap(),
            openpanel_domain::Email::new("admin_user@example.com").unwrap(),
            openpanel_domain::Password::hash("correct horse battery staple").unwrap(),
            Role::Admin,
        )
    }

    #[tokio::test]
    async fn enable_stream_persists_and_audits() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let streams_repo: Arc<dyn BinlogStreamRepository> = Arc::new(
            crate::db_pitr::repo::SqliteBinlogStreamRepository::new(pool.clone()),
        );
        let restores_repo: Arc<dyn PitrRepository> = Arc::new(
            crate::db_pitr::repo::SqlitePitrRepository::new(pool.clone()),
        );
        let inc_repo: Arc<dyn IncrementalRepository> = Arc::new(
            crate::db_pitr::repo::SqliteIncrementalRepository::new(pool.clone()),
        );
        let mut audit = MockAudit::new();
        audit
            .expect_record()
            .times(1)
            .withf(|e| matches!(e.action, AuditAction::PitrStreamEnabled))
            .returning(|_| Ok(()));
        let owner_id = Uuid::new_v4();
        let database_id = Uuid::new_v4();
        seed_real_database_row(pool.clone(), database_id, owner_id).await;
        let database_repo = seed_database_repo(database_id, owner_id);
        let sink: Arc<dyn BinlogSink> = Arc::new(MemorySink::default());
        let tailer: Arc<dyn LogTailer> = Arc::new(MemoryTailer);
        let svc = PitrService::new(
            streams_repo,
            restores_repo,
            inc_repo,
            database_repo,
            sink,
            vec![tailer],
            Arc::new(audit),
        );
        let target = StreamTargetId::new(Uuid::new_v4());
        let stream = svc
            .enable_stream(&owner(owner_id), database_id, target)
            .await
            .expect("enable");
        assert_eq!(stream.status(), BinlogStreamStatus::Active);
    }

    #[tokio::test]
    async fn restore_writes_staging_before_promotion() {
        let (svc, owner_id, db_id) = build_test_service().await;
        svc.enable_stream(&owner(owner_id), db_id, StreamTargetId::new(Uuid::new_v4()))
            .await
            .expect("enable");
        let sink = svc.sink();
        let now = Utc::now();
        sink.append(
            db_id,
            &openpanel_domain::BinlogSegment {
                from: LogSeq::from_bytes(vec![0x00]).unwrap(),
                to: LogSeq::from_bytes(vec![0x09]).unwrap(),
                generated_at: now,
                bytes: vec![0xde, 0xad, 0xbe, 0xef],
            },
        )
        .await
        .expect("append");
        let target_ts = now;
        let restore = svc
            .request_restore(
                &owner(owner_id),
                db_id,
                RestoreRequest {
                    timestamp: target_ts,
                    base_backup: None,
                    confirm: false,
                },
            )
            .await
            .expect("request");
        assert_eq!(restore.status(), PitrRestoreStatus::Staged);
        let staging = restore.staging_db_id().expect("staging id");
        assert_ne!(staging, db_id);
        let promoted = svc
            .promote_restore(&owner(owner_id), restore.id())
            .await
            .expect("promote");
        assert_eq!(promoted.status(), PitrRestoreStatus::Promoted);
    }

    #[tokio::test]
    async fn restore_rejects_timestamp_outside_range() {
        let (svc, owner_id, db_id) = build_test_service().await;
        svc.enable_stream(&owner(owner_id), db_id, StreamTargetId::new(Uuid::new_v4()))
            .await
            .expect("enable");
        let r = svc
            .request_restore(
                &owner(owner_id),
                db_id,
                RestoreRequest {
                    timestamp: Utc::now() - chrono::Duration::seconds(1),
                    base_backup: None,
                    confirm: false,
                },
            )
            .await;
        assert!(matches!(
            r,
            Err(RestoreRequestError::TimestampOutOfRange(_))
        ));
    }

    #[tokio::test]
    async fn replaying_same_timestamp_is_idempotent() {
        let (svc, owner_id, db_id) = build_test_service().await;
        svc.enable_stream(&owner(owner_id), db_id, StreamTargetId::new(Uuid::new_v4()))
            .await
            .expect("enable");
        let sink = svc.sink();
        let now = Utc::now();
        sink.append(
            db_id,
            &openpanel_domain::BinlogSegment {
                from: LogSeq::from_bytes(vec![0x00]).unwrap(),
                to: LogSeq::from_bytes(vec![0x09]).unwrap(),
                generated_at: now,
                bytes: vec![0xde, 0xad, 0xbe, 0xef],
            },
        )
        .await
        .expect("append");
        let target_ts = now;
        let req = RestoreRequest {
            timestamp: target_ts,
            base_backup: None,
            confirm: false,
        };
        let r1 = svc
            .request_restore(&owner(owner_id), db_id, req.clone())
            .await
            .expect("first");
        let r2 = svc
            .request_restore(&owner(owner_id), db_id, req)
            .await
            .expect("second");
        assert_eq!(r1.staging_db_id().unwrap(), r2.staging_db_id().unwrap());
        assert_eq!(r1.status(), r2.status());
    }

    #[tokio::test]
    async fn capture_incremental_records_audit() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let streams_repo: Arc<dyn BinlogStreamRepository> = Arc::new(
            crate::db_pitr::repo::SqliteBinlogStreamRepository::new(pool.clone()),
        );
        let restores_repo: Arc<dyn PitrRepository> = Arc::new(
            crate::db_pitr::repo::SqlitePitrRepository::new(pool.clone()),
        );
        let inc_repo: Arc<dyn IncrementalRepository> = Arc::new(
            crate::db_pitr::repo::SqliteIncrementalRepository::new(pool.clone()),
        );
        let mut audit = MockAudit::new();
        audit
            .expect_record()
            .times(1)
            .withf(|e| matches!(e.action, AuditAction::PitrIncrementalCaptured))
            .returning(|_| Ok(()));
        let owner_id = Uuid::new_v4();
        let database_id = Uuid::new_v4();
        seed_real_database_row(pool.clone(), database_id, owner_id).await;
        let database_repo = seed_database_repo(database_id, owner_id);
        let sink: Arc<dyn BinlogSink> = Arc::new(MemorySink::default());
        let tailer: Arc<dyn LogTailer> = Arc::new(MemoryTailer);
        let svc = PitrService::new(
            streams_repo,
            restores_repo,
            inc_repo,
            database_repo,
            sink,
            vec![tailer],
            Arc::new(audit),
        );
        let inc = svc
            .capture_incremental(
                &owner(owner_id),
                database_id,
                Uuid::new_v4(),
                "s3://bucket/inc/abc",
                IncrementalMode::BlockDelta,
                4096,
                LogSeq::from_bytes(vec![0x05]).unwrap(),
            )
            .await
            .expect("capture");
        assert_eq!(inc.bytes(), 4096);
    }

    #[tokio::test]
    async fn non_owner_cannot_enable_stream() {
        let (svc, _owner_id, db_id) = build_test_service().await;
        let r = svc
            .enable_stream(
                &reseller(Uuid::new_v4()),
                db_id,
                StreamTargetId::new(Uuid::new_v4()),
            )
            .await;
        assert!(matches!(r, Err(RestoreRequestError::Forbidden)));
    }
}
