//! Two-factor authentication service: TOTP enrollment, verification,
//! recovery codes, and the login state machine.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    RepoError,
    identity::{
        Factor, FactorError, FactorKind, RecoveryCode, RecoveryCodeSet, TotpSecret,
        TwoFactorChallenge, repository::FactorRepository,
    },
};
use uuid::Uuid;

use crate::databases::crypto;

/// Errors raised by the two-factor service.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum TwoFactorError {
    /// The user has no enabled factor.
    #[error("no enrolled factor")]
    NoFactor,
    /// The presented code did not match.
    #[error("invalid factor code")]
    InvalidCode,
    /// The presented code was already used (replay protection).
    #[error("factor code replayed")]
    CodeReplayed,
    /// The factor has been revoked.
    #[error("factor has been revoked")]
    Revoked,
    /// The factor does not belong to the user.
    #[error("factor not found for user")]
    FactorNotFound,
    /// The user has no remaining recovery codes.
    #[error("no recovery codes remain")]
    NoRecoveryCodes,
    /// The presentation is malformed.
    #[error("invalid request: {0}")]
    Invalid(String),
    /// Storage failure.
    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<RepoError> for TwoFactorError {
    fn from(e: RepoError) -> Self {
        TwoFactorError::Storage(e.0)
    }
}

/// A submitted factor response during the login challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactorResponse {
    /// TOTP 6-digit code.
    Totp(String),
    /// Recovery code (Crockford base32, three groups of four).
    Recovery(String),
}

/// The view of a pending-login challenge, returned to the browser.
#[derive(Debug, Clone)]
pub struct ChallengeView {
    /// Stable opaque identifier the browser echoes back to verify.
    pub challenge_id: Uuid,
    /// Plaintext token held by the browser (cookie or form field);
    /// not persisted.
    pub challenge_token: String,
    /// When the challenge expires and is no longer usable.
    pub expires_at: DateTime<Utc>,
}

/// Result of a successful TOTP enrollment.
#[derive(Debug, Clone)]
pub struct TotpEnrollment {
    /// The freshly created factor.
    pub factor: Factor,
    /// The plaintext TOTP secret (shown once, never persisted).
    pub secret: TotpSecret,
    /// The otpauth:// URI for QR provisioning.
    pub provisioning_uri: String,
    /// The plaintext recovery codes (shown once, never persisted).
    pub recovery_codes: Vec<String>,
}

/// Two-factor service.
pub struct TwoFactorService {
    factors: Arc<dyn FactorRepository>,
    master_key: [u8; 32],
    audit: Arc<dyn AuditService>,
}

impl TwoFactorService {
    /// Construct a new service. The master key encrypts TOTP secrets at
    /// rest using the same AES-256-GCM envelope as database passwords.
    pub fn new(
        factors: Arc<dyn FactorRepository>,
        master_key: [u8; 32],
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            factors,
            master_key,
            audit,
        }
    }

    /// The encrypted TOTP secret for the user's active factor, if any.
    pub async fn active_totp_factor(
        &self,
        user_id: Uuid,
    ) -> Result<Option<(Factor, TotpSecret)>, TwoFactorError> {
        let Some(factor) = self.factors.find_active_totp_factor(user_id).await? else {
            return Ok(None);
        };
        let Some(encrypted) = self.factors.find_totp_secret_encrypted(factor.id()).await? else {
            return Ok(None);
        };
        let plaintext = crypto::decrypt_from_storage(&self.master_key, &encrypted)
            .map_err(|e| TwoFactorError::Storage(format!("decrypt: {e}")))?;
        let bytes = hex::decode(&plaintext)
            .map_err(|e| TwoFactorError::Storage(format!("hex decode: {e}")))?;
        let secret = TotpSecret::from_bytes(bytes)
            .map_err(|e| TwoFactorError::Storage(format!("invalid secret: {e}")))?;
        Ok(Some((factor, secret)))
    }

    /// Enroll a new TOTP factor (the first factor for the user also
    /// generates the recovery code set). The secret and recovery codes
    /// are returned to the caller exactly once.
    pub async fn enroll_totp(
        &self,
        user_id: Uuid,
        actor: &str,
        issuer: &str,
        account_name: &str,
        now: DateTime<Utc>,
    ) -> Result<TotpEnrollment, TwoFactorError> {
        let secret = TotpSecret::generate();
        let secret_hex = hex::encode(secret.as_bytes());
        let encrypted = crypto::encrypt_to_storage(&self.master_key, &secret_hex)
            .map_err(|e| TwoFactorError::Storage(format!("encrypt: {e}")))?;
        let factor = Factor::new_verified(Uuid::new_v4(), user_id, FactorKind::Totp, now);
        self.factors.insert_factor(&factor, &encrypted).await?;

        // (Re)generate the recovery codes every time a new factor is enrolled.
        let (_set, plaintext_codes) = RecoveryCodeSet::generate(user_id, 10, now);
        let entries = hash_plaintext_codes(&plaintext_codes, now)?;
        self.factors
            .replace_recovery_codes(user_id, &entries)
            .await?;

        let provisioning_uri = secret.provisioning_uri(issuer, account_name);

        self.audit
            .record(AuditEvent::new(
                actor.to_string(),
                AuditAction::TwoFactorEnrolled,
                AuditOutcome::Success,
            ))
            .await
            .map_err(|e| TwoFactorError::Storage(format!("audit: {e}")))?;

        Ok(TotpEnrollment {
            factor,
            secret,
            provisioning_uri,
            recovery_codes: plaintext_codes,
        })
    }

    /// List a user's factors (active and revoked).
    pub async fn list_factors(&self, user_id: Uuid) -> Result<Vec<Factor>, TwoFactorError> {
        Ok(self.factors.list_factors_for_user(user_id).await?)
    }

    /// Whether the user has at least one active factor.
    pub async fn has_active_factor(&self, user_id: Uuid) -> Result<bool, TwoFactorError> {
        Ok(self
            .factors
            .find_active_totp_factor(user_id)
            .await?
            .is_some())
    }

    /// Count the user's remaining recovery codes.
    pub async fn count_recovery_codes(&self, user_id: Uuid) -> Result<u8, TwoFactorError> {
        let entries = self.factors.list_recovery_codes(user_id).await?;
        Ok(entries
            .into_iter()
            .filter(|(_, consumed)| consumed.is_none())
            .count() as u8)
    }

    /// Revoke a factor belonging to the user. The caller must be the
    /// owner of the factor or an Owner-role actor.
    pub async fn revoke_factor(
        &self,
        actor: &str,
        user_id: Uuid,
        factor_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), TwoFactorError> {
        let Some(mut factor) = self.factors.find_factor(factor_id).await? else {
            return Err(TwoFactorError::FactorNotFound);
        };
        if factor.user_id() != user_id {
            return Err(TwoFactorError::FactorNotFound);
        }
        if factor.revoked_at().is_some() {
            return Ok(());
        }
        factor.revoke(now);
        self.factors.update_factor(&factor).await?;
        self.audit
            .record(AuditEvent::new(
                actor.to_string(),
                AuditAction::TwoFactorRevoked,
                AuditOutcome::Success,
            ))
            .await
            .map_err(|e| TwoFactorError::Storage(format!("audit: {e}")))?;
        Ok(())
    }

    /// Regenerate the user's recovery codes. Returns the new plaintexts.
    pub async fn regenerate_recovery_codes(
        &self,
        actor: &str,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Vec<String>, TwoFactorError> {
        let (_set, plaintext_codes) = RecoveryCodeSet::generate(user_id, 10, now);
        let entries = hash_plaintext_codes(&plaintext_codes, now)?;
        self.factors
            .replace_recovery_codes(user_id, &entries)
            .await?;
        self.audit
            .record(AuditEvent::new(
                actor.to_string(),
                AuditAction::TwoFactorRevoked,
                AuditOutcome::Success,
            ))
            .await
            .map_err(|e| TwoFactorError::Storage(format!("audit: {e}")))?;
        Ok(plaintext_codes)
    }

    // --- Login challenges

    /// Issue a new pending-login challenge for a user that has at least
    /// one active factor. The plaintext token is returned once.
    pub async fn issue_login_challenge(
        &self,
        user_id: Uuid,
        source_ip: Option<String>,
        user_agent: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ChallengeView, TwoFactorError> {
        let (challenge, plaintext) =
            TwoFactorChallenge::create(user_id, source_ip, user_agent, now);
        self.factors.insert_challenge(&challenge).await?;
        Ok(ChallengeView {
            challenge_id: challenge.id(),
            challenge_token: plaintext,
            expires_at: challenge.expires_at(),
        })
    }

    /// Verify a factor response against a pending-login challenge. On
    /// success, marks the challenge consumed and returns the user id.
    pub async fn verify_login_challenge(
        &self,
        actor: &str,
        challenge_id: Uuid,
        challenge_token: &str,
        response: FactorResponse,
        now: DateTime<Utc>,
    ) -> Result<Uuid, TwoFactorError> {
        // Verify the challenge token matches.
        if !self
            .factors
            .verify_challenge_token(challenge_id, challenge_token)
            .await?
        {
            return Err(TwoFactorError::InvalidCode);
        }
        let Some(challenge) = self.factors.find_challenge(challenge_id).await? else {
            return Err(TwoFactorError::InvalidCode);
        };
        if !challenge.is_usable(now) {
            return Err(TwoFactorError::CodeReplayed);
        }
        let user_id = challenge.user_id();

        let verify_result = match response {
            FactorResponse::Totp(code) => self.verify_totp_code(user_id, &code, now).await,
            FactorResponse::Recovery(code) => self.verify_recovery_code(user_id, &code, now).await,
        };
        match verify_result {
            Ok(kind) => {
                self.factors.consume_challenge(challenge_id, now).await?;
                self.audit
                    .record(
                        AuditEvent::new(
                            actor.to_string(),
                            AuditAction::TwoFactorVerified,
                            AuditOutcome::Success,
                        )
                        .metadata(serde_json::json!({ "kind": kind })),
                    )
                    .await
                    .map_err(|e| TwoFactorError::Storage(format!("audit: {e}")))?;
                Ok(user_id)
            }
            Err(error) => {
                self.audit
                    .record(
                        AuditEvent::new(
                            actor.to_string(),
                            AuditAction::TwoFactorFailed,
                            AuditOutcome::Failure,
                        )
                        .metadata(serde_json::json!({
                            "reason": error.to_string(),
                        })),
                    )
                    .await
                    .map_err(|e| TwoFactorError::Storage(format!("audit: {e}")))?;
                Err(error)
            }
        }
    }

    async fn verify_totp_code(
        &self,
        user_id: Uuid,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<&'static str, TwoFactorError> {
        let Some((mut factor, secret)) = self.active_totp_factor(user_id).await? else {
            return Err(TwoFactorError::NoFactor);
        };
        let step = secret.verify(code, now).map_err(map_factor_error)?;
        factor.record_used_step(step).map_err(map_factor_error)?;
        self.factors.update_factor(&factor).await?;
        Ok("totp")
    }

    async fn verify_recovery_code(
        &self,
        user_id: Uuid,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<&'static str, TwoFactorError> {
        let consumed = self
            .factors
            .consume_recovery_code(user_id, code, now)
            .await?;
        if consumed {
            Ok("recovery")
        } else {
            Err(TwoFactorError::InvalidCode)
        }
    }
}

fn hash_plaintext_codes(
    plaintexts: &[String],
    created_at: DateTime<Utc>,
) -> Result<Vec<(String, DateTime<Utc>)>, TwoFactorError> {
    let mut entries = Vec::with_capacity(plaintexts.len());
    for code in plaintexts {
        let hash = RecoveryCode::hash(code).map_err(|e| TwoFactorError::Storage(e.to_string()))?;
        entries.push((hash, created_at));
    }
    Ok(entries)
}

fn map_factor_error(e: FactorError) -> TwoFactorError {
    match e {
        FactorError::CodeReplayed => TwoFactorError::CodeReplayed,
        FactorError::Revoked => TwoFactorError::Revoked,
        FactorError::InvalidCodeFormat | FactorError::CodeMismatch => TwoFactorError::InvalidCode,
        FactorError::RecoveryMismatch | FactorError::RecoveryConsumed => {
            TwoFactorError::InvalidCode
        }
        FactorError::InvalidSecret => TwoFactorError::Storage("invalid secret".into()),
        FactorError::NotVerified => TwoFactorError::InvalidCode,
    }
}
