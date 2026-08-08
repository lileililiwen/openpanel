//! `mockall`-based mocks for every port in `openpanel-domain` and
//! `openpanel-core::AuditService`.

use mockall::mock;
use openpanel_core::{AuditEvent, AuditService, CoreResult};
use openpanel_domain::{
    DatabaseRepository, RepoError, SessionRepository, SiteRepository, UserRepository,
};

mock! {
    pub Audit {}

    #[async_trait::async_trait]
    impl AuditService for Audit {
        async fn record(&self, event: AuditEvent) -> CoreResult<()>;
        async fn recent(&self, limit: i64) -> CoreResult<Vec<AuditEvent>>;
    }
}

mock! {
    pub UserRepo {}

    #[async_trait::async_trait]
    impl UserRepository for UserRepo {
        async fn insert(&self, user: &openpanel_domain::User) -> Result<(), RepoError>;
        async fn find_by_id(&self, id: uuid::Uuid) -> Result<Option<openpanel_domain::User>, RepoError>;
        async fn find_by_username(&self, username: &str) -> Result<Option<openpanel_domain::User>, RepoError>;
        async fn find_by_email(&self, email: &str) -> Result<Option<openpanel_domain::User>, RepoError>;
        async fn list(&self) -> Result<Vec<openpanel_domain::User>, RepoError>;
        async fn update_role(&self, id: uuid::Uuid, role: openpanel_domain::Role) -> Result<(), RepoError>;
        async fn disable(&self, id: uuid::Uuid) -> Result<(), RepoError>;
        async fn update_last_login(&self, id: uuid::Uuid) -> Result<(), RepoError>;
        async fn update_password(&self, id: uuid::Uuid, hash: &str) -> Result<(), RepoError>;
        async fn delete(&self, id: uuid::Uuid) -> Result<(), RepoError>;
        async fn count(&self) -> Result<i64, RepoError>;
    }
}

mock! {
    pub SessionRepo {}

    #[async_trait::async_trait]
    impl SessionRepository for SessionRepo {
        async fn insert(&self, session: &openpanel_domain::Session) -> Result<(), RepoError>;
        async fn find_by_token_hash_match(
            &self,
            token: &openpanel_domain::SessionToken,
        ) -> Result<Option<openpanel_domain::Session>, RepoError>;
        async fn find_by_id(&self, id: uuid::Uuid) -> Result<Option<openpanel_domain::Session>, RepoError>;
        async fn touch(&self, id: uuid::Uuid) -> Result<(), RepoError>;
        async fn delete(&self, id: uuid::Uuid) -> Result<(), RepoError>;
        async fn delete_for_user(&self, user_id: uuid::Uuid) -> Result<(), RepoError>;
        async fn purge_expired(&self) -> Result<u64, RepoError>;
    }
}

mock! {
    pub SiteRepo {}

    #[async_trait::async_trait]
    impl SiteRepository for SiteRepo {
        async fn insert(&self, site: &openpanel_domain::Site) -> Result<(), RepoError>;
        async fn find_by_id(&self, id: uuid::Uuid) -> Result<Option<openpanel_domain::Site>, RepoError>;
        async fn find_by_domain(&self, domain: &str) -> Result<Option<openpanel_domain::Site>, RepoError>;
        async fn list_all(&self) -> Result<Vec<openpanel_domain::Site>, RepoError>;
        async fn list_by_owner(&self, owner_id: uuid::Uuid) -> Result<Vec<openpanel_domain::Site>, RepoError>;
        async fn update_status(
            &self,
            id: uuid::Uuid,
            status: openpanel_domain::SiteStatus,
            by: &str,
        ) -> Result<(), RepoError>;
        async fn update_owner(&self, id: uuid::Uuid, owner_id: uuid::Uuid, by: &str) -> Result<(), RepoError>;
        async fn update_aliases(&self, id: uuid::Uuid, aliases_json: &str, by: &str) -> Result<(), RepoError>;
        async fn delete(&self, id: uuid::Uuid) -> Result<(), RepoError>;
        async fn count(&self) -> Result<i64, RepoError>;
    }
}

mock! {
    pub DatabaseRepo {}

    #[async_trait::async_trait]
    impl DatabaseRepository for DatabaseRepo {
        async fn insert(&self, db: &openpanel_domain::Database, password_ciphertext: &str) -> Result<(), RepoError>;
        async fn find_by_id(&self, id: uuid::Uuid) -> Result<Option<openpanel_domain::Database>, RepoError>;
        async fn find_by_name(&self, name: &str) -> Result<Option<openpanel_domain::Database>, RepoError>;
        async fn list_all(&self) -> Result<Vec<openpanel_domain::Database>, RepoError>;
        async fn list_by_owner(&self, owner_id: uuid::Uuid) -> Result<Vec<openpanel_domain::Database>, RepoError>;
        async fn password_ciphertext(&self, id: uuid::Uuid) -> Result<Option<String>, RepoError>;
        async fn update_password_ciphertext(&self, id: uuid::Uuid, ciphertext: &str) -> Result<(), RepoError>;
        async fn update_status(&self, id: uuid::Uuid, status: openpanel_domain::DatabaseStatus) -> Result<(), RepoError>;
        async fn delete(&self, id: uuid::Uuid) -> Result<(), RepoError>;
        async fn count(&self) -> Result<i64, RepoError>;
    }
}

/// Demonstrates `mockall` mock expectations against `MockAudit`.
/// `mockall` checks expectations when the mock is dropped: meeting the
/// expectations exactly is a no-op; over- or under-meeting panics.
#[cfg(test)]
mod tests {
    use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};

    use super::MockAudit;

    fn login_event() -> AuditEvent {
        AuditEvent::new("alice", AuditAction::Login, AuditOutcome::Success)
    }

    /// Met expectation: exactly one `record` call when `times(1)` is
    /// set. Dropping the mock without panicking proves the expectation
    /// was satisfied.
    #[tokio::test]
    async fn mock_audit_records_called_with_matching_event() {
        let mut mock = MockAudit::new();
        mock.expect_record().times(1).returning(|_| Ok(()));
        mock.expect_recent().returning(|_| Ok(vec![]));

        mock.record(login_event()).await.unwrap();
        // Drop checks expectations here; no panic = pass.
    }

    /// Over-met expectation: two `record` calls when `times(1)` is set.
    /// mockall panics on Drop when expectations are violated.
    #[tokio::test]
    #[should_panic(expected = "Expectation")]
    async fn mock_audit_over_met_expectation_panics_on_drop() {
        let mut mock = MockAudit::new();
        mock.expect_record().times(1).returning(|_| Ok(()));
        mock.record(login_event()).await.unwrap();
        mock.record(login_event()).await.unwrap();
        // Drop triggers expectation check and panics.
    }
}
