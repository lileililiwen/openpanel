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

impl MockAudit {
    /// Build a mock whose `record`/`recent` are stubbed to accept any
    /// input and return `Ok`. Use when a test only cares about another
    /// port's calls.
    pub fn stub() -> Self {
        let mut mock = Self::new();
        mock.expect_record().returning(|_| Ok(()));
        mock.expect_recent().returning(|_| Ok(vec![]));
        mock
    }
}

mock! {
    pub SnapshotRepo {}

    #[async_trait::async_trait]
    impl openpanel_domain::SnapshotRepository for SnapshotRepo {
        async fn insert(&self, sample: &openpanel_domain::MetricSample) -> Result<(), openpanel_domain::RepoError>;
        async fn latest(&self) -> Result<Vec<openpanel_domain::MetricSample>, openpanel_domain::RepoError>;
        async fn history(
            &self,
            kind: openpanel_domain::MetricKind,
            since: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<openpanel_domain::MetricSample>, openpanel_domain::RepoError>;
        async fn prune(&self, before: chrono::DateTime<chrono::Utc>) -> Result<u64, openpanel_domain::RepoError>;
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
        async fn enable(&self, id: uuid::Uuid) -> Result<(), RepoError>;
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
    pub FactorRepo {}

    #[async_trait::async_trait]
    #[allow(clippy::type_complexity)]
    impl openpanel_domain::identity::repository::FactorRepository for FactorRepo {
        async fn insert_factor(
            &self,
            factor: &openpanel_domain::identity::Factor,
            totp_secret_encrypted: &str,
        ) -> Result<(), RepoError>;
        async fn find_factor(&self, id: uuid::Uuid) -> Result<Option<openpanel_domain::identity::Factor>, RepoError>;
        async fn update_factor(&self, factor: &openpanel_domain::identity::Factor) -> Result<(), RepoError>;
        async fn list_factors_for_user(&self, user_id: uuid::Uuid) -> Result<Vec<openpanel_domain::identity::Factor>, RepoError>;
        async fn find_active_totp_factor(
            &self,
            user_id: uuid::Uuid,
        ) -> Result<Option<openpanel_domain::identity::Factor>, RepoError>;
        async fn find_totp_secret_encrypted(
            &self,
            factor_id: uuid::Uuid,
        ) -> Result<Option<String>, RepoError>;
        async fn insert_totp_enrollment(
            &self,
            enrollment: &openpanel_domain::identity::TotpEnrollmentChallenge,
        ) -> Result<(), RepoError>;
        async fn find_totp_enrollment(
            &self,
            id: uuid::Uuid,
        ) -> Result<Option<openpanel_domain::identity::TotpEnrollmentChallenge>, RepoError>;
        async fn consume_totp_enrollment(
            &self,
            id: uuid::Uuid,
            now: chrono::DateTime<chrono::Utc>,
        ) -> Result<bool, RepoError>;
        async fn replace_recovery_codes(
            &self,
            user_id: uuid::Uuid,
            entries: &[(String, chrono::DateTime<chrono::Utc>)],
        ) -> Result<(), RepoError>;
        async fn list_recovery_codes(
            &self,
            user_id: uuid::Uuid,
        ) -> Result<Vec<(String, Option<chrono::DateTime<chrono::Utc>>)>, RepoError>;
        async fn consume_recovery_code(
            &self,
            user_id: uuid::Uuid,
            plaintext: &str,
            now: chrono::DateTime<chrono::Utc>,
        ) -> Result<bool, RepoError>;
        async fn insert_challenge(
            &self,
            challenge: &openpanel_domain::identity::TwoFactorChallenge,
        ) -> Result<(), RepoError>;
        async fn find_challenge(
            &self,
            id: uuid::Uuid,
        ) -> Result<Option<openpanel_domain::identity::TwoFactorChallenge>, RepoError>;
        async fn delete_challenge(&self, id: uuid::Uuid) -> Result<(), RepoError>;
        async fn verify_challenge_token(
            &self,
            challenge_id: uuid::Uuid,
            plaintext: &str,
        ) -> Result<bool, RepoError>;
        async fn consume_challenge(
            &self,
            challenge_id: uuid::Uuid,
            now: chrono::DateTime<chrono::Utc>,
        ) -> Result<Option<uuid::Uuid>, RepoError>;
        async fn purge_expired_challenges(
            &self,
            now: chrono::DateTime<chrono::Utc>,
        ) -> Result<u64, RepoError>;
        async fn insert_webauthn_credential<'a>(
            &self,
            credential_id: &str,
            user_id: uuid::Uuid,
            factor_id: uuid::Uuid,
            public_key_spki: &str,
            sign_count: i64,
            transports: Option<&'a str>,
            uv_policy: &str,
            passkey_json: &str,
            created_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), RepoError>;
        async fn list_webauthn_credentials(
            &self,
            user_id: uuid::Uuid,
        ) -> Result<Vec<openpanel_domain::identity::WebAuthnCredential>, RepoError>;
        async fn find_webauthn_credential_by_id(
            &self,
            credential_id: &str,
        ) -> Result<Option<openpanel_domain::identity::WebAuthnCredential>, RepoError>;
        async fn touch_webauthn_credential(
            &self,
            credential_id: &str,
            new_sign_count: i64,
            last_used_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), RepoError>;
        async fn revoke_webauthn_credential(
            &self,
            credential_id: &str,
            now: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), RepoError>;
        async fn insert_webauthn_challenge(
            &self,
            id: uuid::Uuid,
            user_id: uuid::Uuid,
            kind: &str,
            state_json: &str,
            created_at: chrono::DateTime<chrono::Utc>,
            expires_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), RepoError>;
        async fn find_webauthn_challenge(
            &self,
            id: uuid::Uuid,
        ) -> Result<Option<openpanel_domain::identity::WebAuthnChallenge>, RepoError>;
        async fn consume_webauthn_challenge(
            &self,
            id: uuid::Uuid,
            now: chrono::DateTime<chrono::Utc>,
        ) -> Result<Option<openpanel_domain::identity::WebAuthnChallenge>, RepoError>;
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
