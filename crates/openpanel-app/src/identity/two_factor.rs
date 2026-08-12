//! Two-factor authentication service: TOTP enrollment, verification,
//! recovery codes, WebAuthn ceremony, and the login state machine.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    RepoError,
    identity::{
        Factor, FactorError, FactorKind, RecoveryCode, RecoveryCodeSet, TotpEnrollmentChallenge,
        TotpSecret, TwoFactorChallenge, UserVerificationPolicy, WebAuthnChallenge,
        repository::FactorRepository,
    },
};
use uuid::Uuid;
use webauthn_rs::prelude::*;

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
    /// The remember-device cookie was missing, malformed, or its HMAC
    /// did not verify.
    #[error("remember-device cookie invalid")]
    RememberDeviceInvalid,
    /// The remember-device cookie's lifetime has elapsed.
    #[error("remember-device cookie expired")]
    RememberDeviceExpired,
    /// The remember-device cookie's UA hash or IP prefix does not
    /// match the current request.
    #[error("remember-device cookie does not match this device")]
    RememberDeviceMismatch,
    /// WebAuthn is not configured on this panel (missing RP id or
    /// origin).
    #[error("WebAuthn is not configured")]
    WebAuthnUnavailable,
    /// A WebAuthn ceremony operation failed (challenge generation,
    /// state persistence, or browser-response verification).
    #[error("WebAuthn ceremony: {0}")]
    WebAuthnCeremony(String),
    /// A WebAuthn ceremony challenge has expired or was already consumed.
    #[error("WebAuthn ceremony expired")]
    WebAuthnCeremonyExpired,
}

impl From<RepoError> for TwoFactorError {
    fn from(e: RepoError) -> Self {
        TwoFactorError::Storage(e.0)
    }
}

/// A submitted factor response during the login challenge.
#[derive(Debug, Clone)]
pub enum FactorResponse {
    /// TOTP 6-digit code.
    Totp(String),
    /// Recovery code (Crockford base32, three groups of four).
    Recovery(String),
    /// WebAuthn assertion JSON (deserialized from the browser's
    /// `navigator.credentials.get()` response).
    WebAuthn {
        /// Panel-side id of the persisted WebAuthn assertion ceremony.
        ceremony_challenge_id: Uuid,
        /// Browser assertion response.
        credential: serde_json::Value,
    },
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
    /// Pending enrollment id that must be verified before activation.
    pub enrollment_id: Uuid,
    /// The plaintext TOTP secret (shown once, never persisted).
    pub secret: TotpSecret,
    /// The otpauth:// URI for QR provisioning.
    pub provisioning_uri: String,
}

/// Result of successfully proving possession of a pending TOTP secret.
#[derive(Debug, Clone)]
pub struct VerifiedTotpEnrollment {
    /// Newly activated TOTP factor.
    pub factor: Factor,
    /// Fresh recovery codes shown once after activation.
    pub recovery_codes: Vec<String>,
}

/// Verified material returned by a WebAuthn registration ceremony.
#[derive(Debug, Clone)]
pub struct RegisteredWebAuthnCredential {
    credential_id: Vec<u8>,
    public_key: String,
    sign_count: i64,
    transports: Option<String>,
    passkey_json: String,
}

impl RegisteredWebAuthnCredential {
    /// Construct verified registration material from a crypto adapter.
    pub fn new(
        credential_id: Vec<u8>,
        public_key: String,
        sign_count: i64,
        transports: Option<String>,
        passkey_json: String,
    ) -> Self {
        Self {
            credential_id,
            public_key,
            sign_count,
            transports,
            passkey_json,
        }
    }
}

/// Verified material returned by a WebAuthn assertion ceremony.
#[derive(Debug, Clone)]
pub struct VerifiedWebAuthnAssertion {
    credential_id: Vec<u8>,
    sign_count: i64,
}

impl VerifiedWebAuthnAssertion {
    /// Construct verified assertion material from a crypto adapter.
    pub fn new(credential_id: Vec<u8>, sign_count: i64) -> Self {
        Self {
            credential_id,
            sign_count,
        }
    }
}

/// Cryptographic boundary for TOTP generation and WebAuthn ceremonies.
/// Tests can replace this with deterministic ceremony results.
pub trait TwoFactorCrypto: Send + Sync + 'static {
    /// Generate a new TOTP secret.
    fn generate_totp_secret(&self) -> TotpSecret;
    /// Begin passkey registration, returning browser options and opaque state.
    fn begin_webauthn_register(
        &self,
        user_id: Uuid,
        username: &str,
    ) -> Result<(serde_json::Value, String), TwoFactorError>;
    /// Verify a browser registration response.
    fn finish_webauthn_register(
        &self,
        response: &serde_json::Value,
        state_json: &str,
    ) -> Result<RegisteredWebAuthnCredential, TwoFactorError>;
    /// Begin passkey assertion, returning browser options and opaque state.
    fn begin_webauthn_assert(
        &self,
        passkeys_json: &[String],
    ) -> Result<(serde_json::Value, String), TwoFactorError>;
    /// Verify a browser assertion response.
    fn finish_webauthn_assert(
        &self,
        response: &serde_json::Value,
        state_json: &str,
    ) -> Result<VerifiedWebAuthnAssertion, TwoFactorError>;
}

struct RustTwoFactorCrypto {
    webauthn: Option<Webauthn>,
}

impl TwoFactorCrypto for RustTwoFactorCrypto {
    fn generate_totp_secret(&self) -> TotpSecret {
        TotpSecret::generate()
    }

    fn begin_webauthn_register(
        &self,
        user_id: Uuid,
        username: &str,
    ) -> Result<(serde_json::Value, String), TwoFactorError> {
        let webauthn = self
            .webauthn
            .as_ref()
            .ok_or(TwoFactorError::WebAuthnUnavailable)?;
        let user_unique_id = webauthn_rs::prelude::Uuid::from_bytes(user_id.into_bytes());
        let (challenge, state) = webauthn
            .start_passkey_registration(user_unique_id, username, username, None)
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        Ok((
            serde_json::to_value(challenge)
                .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?,
            serde_json::to_string(&state)
                .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?,
        ))
    }

    fn finish_webauthn_register(
        &self,
        response: &serde_json::Value,
        state_json: &str,
    ) -> Result<RegisteredWebAuthnCredential, TwoFactorError> {
        let webauthn = self
            .webauthn
            .as_ref()
            .ok_or(TwoFactorError::WebAuthnUnavailable)?;
        let response: RegisterPublicKeyCredential = serde_json::from_value(response.clone())
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        let state: PasskeyRegistration = serde_json::from_str(state_json)
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        let passkey = webauthn
            .finish_passkey_registration(&response, &state)
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        let credential: Credential = passkey.clone().into();
        let public_key = serde_json::to_string(&credential.cred)
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        let transports = credential.transports.as_ref().map(|values| {
            values
                .iter()
                .map(|value| format!("{value:?}").to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join(",")
        });
        Ok(RegisteredWebAuthnCredential {
            credential_id: credential.cred_id.as_slice().to_vec(),
            public_key,
            sign_count: i64::from(credential.counter),
            transports,
            passkey_json: serde_json::to_string(&passkey)
                .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?,
        })
    }

    fn begin_webauthn_assert(
        &self,
        passkeys_json: &[String],
    ) -> Result<(serde_json::Value, String), TwoFactorError> {
        let webauthn = self
            .webauthn
            .as_ref()
            .ok_or(TwoFactorError::WebAuthnUnavailable)?;
        let passkeys = passkeys_json
            .iter()
            .map(|value| serde_json::from_str(value))
            .collect::<Result<Vec<Passkey>, _>>()
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        let (challenge, state) = webauthn
            .start_passkey_authentication(&passkeys)
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        Ok((
            serde_json::to_value(challenge)
                .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?,
            serde_json::to_string(&state)
                .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?,
        ))
    }

    fn finish_webauthn_assert(
        &self,
        response: &serde_json::Value,
        state_json: &str,
    ) -> Result<VerifiedWebAuthnAssertion, TwoFactorError> {
        let webauthn = self
            .webauthn
            .as_ref()
            .ok_or(TwoFactorError::WebAuthnUnavailable)?;
        let response: PublicKeyCredential = serde_json::from_value(response.clone())
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        let state: PasskeyAuthentication = serde_json::from_str(state_json)
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        let result = webauthn
            .finish_passkey_authentication(&response, &state)
            .map_err(|error| TwoFactorError::WebAuthnCeremony(error.to_string()))?;
        Ok(VerifiedWebAuthnAssertion {
            credential_id: result.cred_id().as_slice().to_vec(),
            sign_count: i64::from(result.counter()),
        })
    }
}

/// Two-factor service.
pub struct TwoFactorService {
    factors: Arc<dyn FactorRepository>,
    master_key: [u8; 32],
    audit: Arc<dyn AuditService>,
    crypto: Arc<dyn TwoFactorCrypto>,
    settings: TwoFactorSettings,
}

/// Runtime settings for second-factor enrollment and challenges.
#[derive(Debug, Clone)]
pub struct TwoFactorSettings {
    /// Label shown by authenticator applications.
    pub issuer_label: String,
    /// Lifetime of password-login and WebAuthn ceremony challenges.
    pub challenge_lifetime: chrono::Duration,
    /// Number of single-use codes created as a set.
    pub recovery_code_count: u8,
    /// Lifetime of a remembered-device cookie.
    pub remember_device_lifetime: chrono::Duration,
}

impl Default for TwoFactorSettings {
    fn default() -> Self {
        Self {
            issuer_label: "OpenPanel".to_string(),
            challenge_lifetime: chrono::Duration::minutes(5),
            recovery_code_count: 10,
            remember_device_lifetime: chrono::Duration::days(30),
        }
    }
}

impl TwoFactorService {
    // --- WebAuthn ceremony ---

    /// Default lifetime for a pending WebAuthn ceremony challenge.
    pub const WEBAUTHN_CEREMONY_LIFETIME: chrono::Duration = chrono::Duration::minutes(5);

    /// Construct a new service with TOTP + recovery only. The master key
    /// encrypts TOTP secrets at rest using the same AES-256-GCM envelope
    /// as database passwords.
    pub fn new(
        factors: Arc<dyn FactorRepository>,
        master_key: [u8; 32],
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            factors,
            master_key,
            audit,
            crypto: Arc::new(RustTwoFactorCrypto { webauthn: None }),
            settings: TwoFactorSettings::default(),
        }
    }

    /// Construct a service with WebAuthn ceremony enabled. The RP id is
    /// the panel's bare host (e.g. `"openpanel.local"` or
    /// `"panel.example.com"`); the origin is the full URL browsers send
    /// in `navigator.credentials.create/get`.
    pub fn new_with_webauthn(
        factors: Arc<dyn FactorRepository>,
        master_key: [u8; 32],
        audit: Arc<dyn AuditService>,
        rp_id: &str,
        rp_origin: &str,
    ) -> Result<Self, TwoFactorError> {
        Self::new_with_webauthn_origins(factors, master_key, audit, rp_id, &[rp_origin.to_string()])
    }

    /// Construct a service with an explicit WebAuthn origin allowlist.
    pub fn new_with_webauthn_origins(
        factors: Arc<dyn FactorRepository>,
        master_key: [u8; 32],
        audit: Arc<dyn AuditService>,
        rp_id: &str,
        rp_origins: &[String],
    ) -> Result<Self, TwoFactorError> {
        let (first_origin, extra_origins) = rp_origins
            .split_first()
            .ok_or_else(|| TwoFactorError::Invalid("WebAuthn origins cannot be empty".into()))?;
        let first_origin = Url::parse(first_origin)
            .map_err(|error| TwoFactorError::Invalid(format!("WebAuthn origin: {error}")))?;
        let mut builder = WebauthnBuilder::new(rp_id, &first_origin)
            .map_err(|error| TwoFactorError::Invalid(format!("WebAuthn configuration: {error}")))?;
        for origin in extra_origins {
            let origin = Url::parse(origin)
                .map_err(|error| TwoFactorError::Invalid(format!("WebAuthn origin: {error}")))?;
            builder = builder.append_allowed_origin(&origin);
        }
        let webauthn = builder
            .build()
            .map_err(|error| TwoFactorError::Invalid(format!("WebAuthn configuration: {error}")))?;
        Ok(Self {
            factors,
            master_key,
            audit,
            crypto: Arc::new(RustTwoFactorCrypto {
                webauthn: Some(webauthn),
            }),
            settings: TwoFactorSettings::default(),
        })
    }

    /// Construct with an injected cryptographic provider. Intended for
    /// deterministic service and HTTP integration tests.
    pub fn new_with_crypto(
        factors: Arc<dyn FactorRepository>,
        master_key: [u8; 32],
        audit: Arc<dyn AuditService>,
        crypto: Arc<dyn TwoFactorCrypto>,
    ) -> Self {
        Self {
            factors,
            master_key,
            audit,
            crypto,
            settings: TwoFactorSettings::default(),
        }
    }

    /// Apply validated runtime settings supplied by the identity module.
    pub fn with_settings(mut self, settings: TwoFactorSettings) -> Self {
        self.settings = settings;
        self
    }

    /// Login-challenge cookie lifetime in seconds.
    pub fn challenge_lifetime_seconds(&self) -> i64 {
        self.settings.challenge_lifetime.num_seconds()
    }

    /// Remember-device cookie lifetime in seconds.
    pub fn remember_device_lifetime_seconds(&self) -> i64 {
        self.settings.remember_device_lifetime.num_seconds()
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
        account_name: &str,
        now: DateTime<Utc>,
    ) -> Result<TotpEnrollment, TwoFactorError> {
        let secret = self.crypto.generate_totp_secret();
        let secret_hex = hex::encode(secret.as_bytes());
        let encrypted = crypto::encrypt_to_storage(&self.master_key, &secret_hex)
            .map_err(|e| TwoFactorError::Storage(format!("encrypt: {e}")))?;
        let enrollment_id = Uuid::new_v4();
        let enrollment = TotpEnrollmentChallenge::new(enrollment_id, user_id, encrypted, now);
        self.factors.insert_totp_enrollment(&enrollment).await?;
        let provisioning_uri = secret.provisioning_uri(&self.settings.issuer_label, account_name);
        let _ = actor;

        Ok(TotpEnrollment {
            enrollment_id,
            secret,
            provisioning_uri,
        })
    }

    /// Verify possession of a pending TOTP secret, activate the factor,
    /// and generate recovery codes exactly once.
    pub async fn verify_totp_enrollment(
        &self,
        actor: &str,
        user_id: Uuid,
        enrollment_id: Uuid,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<VerifiedTotpEnrollment, TwoFactorError> {
        let enrollment = self
            .factors
            .find_totp_enrollment(enrollment_id)
            .await?
            .filter(|value| value.user_id() == user_id && value.is_usable(now))
            .ok_or(TwoFactorError::InvalidCode)?;
        let plaintext =
            crypto::decrypt_from_storage(&self.master_key, enrollment.encrypted_secret())
                .map_err(|error| TwoFactorError::Storage(format!("decrypt: {error}")))?;
        let secret = TotpSecret::from_bytes(
            hex::decode(&plaintext)
                .map_err(|error| TwoFactorError::Storage(format!("hex decode: {error}")))?,
        )
        .map_err(map_factor_error)?;
        let verified_step = secret.verify(code, now).map_err(map_factor_error)?;
        if !self
            .factors
            .consume_totp_enrollment(enrollment_id, now)
            .await?
        {
            return Err(TwoFactorError::CodeReplayed);
        }
        let mut factor = Factor::new_verified(Uuid::new_v4(), user_id, FactorKind::Totp, now);
        factor
            .record_used_step(verified_step)
            .map_err(map_factor_error)?;
        self.factors
            .insert_factor(&factor, enrollment.encrypted_secret())
            .await?;
        let (_set, plaintext_codes) =
            RecoveryCodeSet::generate(user_id, self.settings.recovery_code_count, now);
        let entries = hash_plaintext_codes(&plaintext_codes, now)?;
        self.factors
            .replace_recovery_codes(user_id, &entries)
            .await?;
        self.audit
            .record(AuditEvent::new(
                actor.to_string(),
                AuditAction::TwoFactorEnrolled,
                AuditOutcome::Success,
            ))
            .await
            .map_err(|error| TwoFactorError::Storage(format!("audit: {error}")))?;
        Ok(VerifiedTotpEnrollment {
            factor,
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
            .list_factors_for_user(user_id)
            .await?
            .into_iter()
            .any(|factor| factor.revoked_at().is_none()))
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
        if factor.kind() == FactorKind::WebAuthn {
            for credential in self.factors.list_webauthn_credentials(user_id).await? {
                if credential.factor_id() == factor_id {
                    self.factors
                        .revoke_webauthn_credential(credential.credential_id(), now)
                        .await?;
                }
            }
        }
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
        let (_set, plaintext_codes) =
            RecoveryCodeSet::generate(user_id, self.settings.recovery_code_count, now);
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
        let (challenge, plaintext) = TwoFactorChallenge::create_with_lifetime(
            user_id,
            source_ip,
            user_agent,
            now,
            self.settings.challenge_lifetime,
        );
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
        let (user_id, _) = self
            .verify_login_challenge_with_factor(actor, challenge_id, challenge_token, response, now)
            .await?;
        Ok(user_id)
    }

    /// Like [`Self::verify_login_challenge`] but also returns the factor id
    /// that satisfied the response. `None` means a recovery code was
    /// used (recovery codes are account-scoped, not factor-scoped).
    pub async fn verify_login_challenge_with_factor(
        &self,
        actor: &str,
        challenge_id: Uuid,
        challenge_token: &str,
        response: FactorResponse,
        now: DateTime<Utc>,
    ) -> Result<(Uuid, Option<Uuid>), TwoFactorError> {
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
            FactorResponse::WebAuthn {
                ceremony_challenge_id,
                credential,
            } => {
                let (asserted_user_id, factor_id) = self
                    .finish_webauthn_assert(actor, ceremony_challenge_id, &credential, now)
                    .await?;
                if asserted_user_id != user_id {
                    Err(TwoFactorError::InvalidCode)
                } else {
                    Ok((Some(factor_id), "webauthn"))
                }
            }
        };
        match verify_result {
            Ok((factor_id, kind)) => {
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
                Ok((user_id, factor_id))
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
    ) -> Result<(Option<Uuid>, &'static str), TwoFactorError> {
        let Some((mut factor, secret)) = self.active_totp_factor(user_id).await? else {
            return Err(TwoFactorError::NoFactor);
        };
        let step = secret.verify(code, now).map_err(map_factor_error)?;
        factor.record_used_step(step).map_err(map_factor_error)?;
        let factor_id = factor.id();
        self.factors.update_factor(&factor).await?;
        Ok((Some(factor_id), "totp"))
    }

    async fn verify_recovery_code(
        &self,
        user_id: Uuid,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<(Option<Uuid>, &'static str), TwoFactorError> {
        let consumed = self
            .factors
            .consume_recovery_code(user_id, code, now)
            .await?;
        if consumed {
            // Recovery codes are account-scoped, not factor-scoped.
            Ok((None, "recovery"))
        } else {
            Err(TwoFactorError::InvalidCode)
        }
    }

    /// Begin a WebAuthn registration ceremony. Returns the panel-side
    /// challenge id and the `CreationChallengeResponse` JSON the browser
    /// must hand to `navigator.credentials.create`.
    pub fn begin_webauthn_register(
        &self,
        user_id: Uuid,
        username: &str,
    ) -> Result<(Uuid, serde_json::Value, String), TwoFactorError> {
        let (challenge, state_json) = self.crypto.begin_webauthn_register(user_id, username)?;
        let challenge_id = Uuid::new_v4();
        Ok((challenge_id, challenge, state_json))
    }

    /// Persist an in-flight registration ceremony state.
    pub async fn persist_webauthn_register(
        &self,
        challenge_id: Uuid,
        user_id: Uuid,
        state_json: &str,
        now: DateTime<Utc>,
    ) -> Result<(), TwoFactorError> {
        let expires_at = now + self.settings.challenge_lifetime;
        self.factors
            .insert_webauthn_challenge(
                challenge_id,
                user_id,
                WebAuthnChallenge::KIND_REGISTER,
                state_json,
                now,
                expires_at,
            )
            .await?;
        Ok(())
    }

    /// Begin a WebAuthn assertion (login) ceremony. Returns the
    /// challenge id and the `RequestChallengeResponse` JSON.
    pub async fn begin_webauthn_assert(
        &self,
        user_id: Uuid,
    ) -> Result<(Uuid, serde_json::Value), TwoFactorError> {
        let creds = self.factors.list_webauthn_credentials(user_id).await?;
        if creds.is_empty() {
            return Err(TwoFactorError::NoFactor);
        }
        let passkeys_json: Vec<String> = creds
            .iter()
            .map(|credential| credential.passkey_json().to_string())
            .collect();
        let (challenge, state_json) = self.crypto.begin_webauthn_assert(&passkeys_json)?;
        let challenge_id = Uuid::new_v4();
        let now = Utc::now();
        let expires_at = now + self.settings.challenge_lifetime;
        self.factors
            .insert_webauthn_challenge(
                challenge_id,
                user_id,
                WebAuthnChallenge::KIND_ASSERT,
                &state_json,
                now,
                expires_at,
            )
            .await?;
        Ok((challenge_id, challenge))
    }

    /// Begin an assertion ceremony for a still-usable password-login
    /// challenge. This binds the WebAuthn ceremony to the account that
    /// successfully completed the password step.
    pub async fn begin_webauthn_login(
        &self,
        login_challenge_id: Uuid,
        login_challenge_token: &str,
        now: DateTime<Utc>,
    ) -> Result<(Uuid, serde_json::Value), TwoFactorError> {
        if !self
            .factors
            .verify_challenge_token(login_challenge_id, login_challenge_token)
            .await?
        {
            return Err(TwoFactorError::InvalidCode);
        }
        let challenge = self
            .factors
            .find_challenge(login_challenge_id)
            .await?
            .ok_or(TwoFactorError::InvalidCode)?;
        if !challenge.is_usable(now) {
            return Err(TwoFactorError::CodeReplayed);
        }
        self.begin_webauthn_assert(challenge.user_id()).await
    }

    /// Complete a WebAuthn registration ceremony. Returns the panel-side
    /// id of the new `WebAuthnCredential` row.
    ///
    /// The crypto round-trip (attestation
    /// verification) is invoked but its inputs are the browser's
    /// `RegisterPublicKeyCredential` JSON — without a real browser
    /// this is exercised only against the malformed/expired paths.
    pub async fn finish_webauthn_register(
        &self,
        actor: &str,
        challenge_id: Uuid,
        user_id: Uuid,
        response: &serde_json::Value,
        now: DateTime<Utc>,
    ) -> Result<Uuid, TwoFactorError> {
        let challenge = self
            .factors
            .consume_webauthn_challenge(challenge_id, now)
            .await?
            .ok_or(TwoFactorError::WebAuthnCeremonyExpired)?;
        if challenge.user_id() != user_id || challenge.kind() != WebAuthnChallenge::KIND_REGISTER {
            return Err(TwoFactorError::InvalidCode);
        }
        let credential = self
            .crypto
            .finish_webauthn_register(response, challenge.state_json())?;
        let credential_id_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            &credential.credential_id,
        );
        let factor_id = Uuid::new_v4();
        let factor = Factor::new_verified(factor_id, user_id, FactorKind::WebAuthn, now);
        self.factors.insert_factor(&factor, "").await?;
        self.factors
            .insert_webauthn_credential(
                &credential_id_b64,
                user_id,
                factor_id,
                &credential.public_key,
                credential.sign_count,
                credential.transports.as_deref(),
                UserVerificationPolicy::default().as_str(),
                &credential.passkey_json,
                now,
            )
            .await?;
        if self.count_recovery_codes(user_id).await? == 0 {
            let (_set, plaintext_codes) =
                RecoveryCodeSet::generate(user_id, self.settings.recovery_code_count, now);
            let entries = hash_plaintext_codes(&plaintext_codes, now)?;
            self.factors
                .replace_recovery_codes(user_id, &entries)
                .await?;
        }
        self.audit
            .record(AuditEvent::new(
                actor.to_string(),
                AuditAction::TwoFactorEnrolled,
                AuditOutcome::Success,
            ))
            .await
            .map_err(|e| TwoFactorError::Storage(format!("audit: {e}")))?;
        Ok(factor_id)
    }

    /// Complete a WebAuthn assertion ceremony. Updates the credential's
    /// `sign_count` and `last_used_at` on success. The crypto round-trip
    /// (signature verification) is invoked but its inputs are the
    /// browser's `PublicKeyCredential` JSON — without a real browser
    /// this is exercised only against the malformed/expired paths.
    pub async fn finish_webauthn_assert(
        &self,
        actor: &str,
        challenge_id: Uuid,
        response: &serde_json::Value,
        now: DateTime<Utc>,
    ) -> Result<(Uuid, Uuid), TwoFactorError> {
        let challenge = self
            .factors
            .consume_webauthn_challenge(challenge_id, now)
            .await?
            .ok_or(TwoFactorError::WebAuthnCeremonyExpired)?;
        if challenge.kind() != WebAuthnChallenge::KIND_ASSERT {
            return Err(TwoFactorError::InvalidCode);
        }
        let result = self
            .crypto
            .finish_webauthn_assert(response, challenge.state_json())?;
        let credential_id_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            &result.credential_id,
        );
        let mut cred = self
            .factors
            .find_webauthn_credential_by_id(&credential_id_b64)
            .await?
            .ok_or(TwoFactorError::InvalidCode)?;
        let new_counter = result.sign_count;
        if let Err(error) = cred.record_assertion(new_counter, now) {
            self.audit
                .record(AuditEvent::new(
                    actor.to_string(),
                    AuditAction::TwoFactorFailed,
                    AuditOutcome::Failure,
                ))
                .await
                .map_err(|error| TwoFactorError::Storage(format!("audit: {error}")))?;
            return Err(map_factor_error(error));
        }
        self.factors
            .touch_webauthn_credential(&credential_id_b64, new_counter, now)
            .await?;
        Ok((cred.user_id(), cred.factor_id()))
    }
}

#[cfg(test)]
// Kept beside the ceremony implementation so mock expectations remain
// readable; remember-device methods below form a separate implementation block.
#[allow(clippy::items_after_test_module)]
mod tests {
    use openpanel_test_support::{MockAudit, MockFactorRepo};

    use super::*;

    struct DeterministicCrypto;

    impl TwoFactorCrypto for DeterministicCrypto {
        fn generate_totp_secret(&self) -> TotpSecret {
            TotpSecret::from_bytes(vec![0x42; 20]).expect("fixed test secret")
        }

        fn begin_webauthn_register(
            &self,
            _user_id: Uuid,
            _username: &str,
        ) -> Result<(serde_json::Value, String), TwoFactorError> {
            Err(TwoFactorError::WebAuthnUnavailable)
        }

        fn finish_webauthn_register(
            &self,
            _response: &serde_json::Value,
            _state_json: &str,
        ) -> Result<RegisteredWebAuthnCredential, TwoFactorError> {
            Err(TwoFactorError::WebAuthnUnavailable)
        }

        fn begin_webauthn_assert(
            &self,
            _passkeys_json: &[String],
        ) -> Result<(serde_json::Value, String), TwoFactorError> {
            Err(TwoFactorError::WebAuthnUnavailable)
        }

        fn finish_webauthn_assert(
            &self,
            _response: &serde_json::Value,
            _state_json: &str,
        ) -> Result<VerifiedWebAuthnAssertion, TwoFactorError> {
            Err(TwoFactorError::WebAuthnUnavailable)
        }
    }

    #[test]
    fn webauthn_constructor_enforces_rp_origin_and_accepts_allowlist() {
        let valid = TwoFactorService::new_with_webauthn_origins(
            Arc::new(MockFactorRepo::new()),
            [7; 32],
            Arc::new(MockAudit::new()),
            "example.com",
            &[
                "https://panel.example.com".into(),
                "https://backup.example.com".into(),
            ],
        );
        assert!(valid.is_ok());

        let invalid = TwoFactorService::new_with_webauthn(
            Arc::new(MockFactorRepo::new()),
            [7; 32],
            Arc::new(MockAudit::new()),
            "example.com",
            "https://attacker.invalid",
        );
        assert!(matches!(invalid, Err(TwoFactorError::Invalid(_))));
    }

    #[tokio::test]
    async fn enroll_totp_persists_only_pending_encrypted_state() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).expect("fixed timestamp");
        let user_id = Uuid::new_v4();
        let mut repo = MockFactorRepo::new();
        repo.expect_insert_totp_enrollment()
            .withf(move |enrollment| {
                enrollment.user_id() == user_id
                    && enrollment.created_at() == now
                    && !enrollment.encrypted_secret().is_empty()
            })
            .times(1)
            .returning(|_| Ok(()));
        let service = TwoFactorService::new_with_crypto(
            Arc::new(repo),
            [7; 32],
            Arc::new(MockAudit::new()),
            Arc::new(DeterministicCrypto),
        );

        let enrollment = service
            .enroll_totp(user_id, "alice", "alice", now)
            .await
            .expect("enroll TOTP");
        assert_eq!(enrollment.secret.as_bytes(), &[0x42; 20]);
    }

    #[tokio::test]
    async fn recovery_verification_consumes_challenge_and_audits_success() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).expect("fixed timestamp");
        let user_id = Uuid::new_v4();
        let (challenge, token) = TwoFactorChallenge::create(user_id, None, None, now);
        let challenge_id = challenge.id();
        let expected_token = token.clone();
        let mut repo = MockFactorRepo::new();
        repo.expect_verify_challenge_token()
            .withf(move |id, value| *id == challenge_id && value == expected_token)
            .times(1)
            .returning(|_, _| Ok(true));
        repo.expect_find_challenge()
            .withf(move |id| *id == challenge_id)
            .times(1)
            .return_once(move |_| Ok(Some(challenge)));
        repo.expect_consume_recovery_code()
            .withf(move |id, code, used_at| {
                *id == user_id && code == "RECOVERY-CODE" && *used_at == now
            })
            .times(1)
            .returning(|_, _, _| Ok(true));
        repo.expect_consume_challenge()
            .withf(move |id, used_at| *id == challenge_id && *used_at == now)
            .times(1)
            .returning(move |_, _| Ok(Some(user_id)));
        let mut audit = MockAudit::new();
        audit
            .expect_record()
            .withf(|event| event.action == AuditAction::TwoFactorVerified)
            .times(1)
            .returning(|_| Ok(()));
        let service = TwoFactorService::new_with_crypto(
            Arc::new(repo),
            [7; 32],
            Arc::new(audit),
            Arc::new(DeterministicCrypto),
        );

        let result = service
            .verify_login_challenge_with_factor(
                "alice",
                challenge_id,
                &token,
                FactorResponse::Recovery("RECOVERY-CODE".into()),
                now,
            )
            .await
            .expect("verify recovery code");
        assert_eq!(result, (user_id, None));
    }

    #[tokio::test]
    async fn revoke_factor_updates_owned_factor_and_audits_actor() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).expect("fixed timestamp");
        let user_id = Uuid::new_v4();
        let factor_id = Uuid::new_v4();
        let factor = Factor::new_verified(factor_id, user_id, FactorKind::Totp, now);
        let mut repo = MockFactorRepo::new();
        repo.expect_find_factor()
            .withf(move |id| *id == factor_id)
            .times(1)
            .return_once(move |_| Ok(Some(factor)));
        repo.expect_update_factor()
            .withf(move |updated| updated.id() == factor_id && updated.revoked_at() == Some(now))
            .times(1)
            .returning(|_| Ok(()));
        let mut audit = MockAudit::new();
        audit
            .expect_record()
            .withf(|event| event.action == AuditAction::TwoFactorRevoked && event.actor == "owner")
            .times(1)
            .returning(|_| Ok(()));
        let service = TwoFactorService::new_with_crypto(
            Arc::new(repo),
            [7; 32],
            Arc::new(audit),
            Arc::new(DeterministicCrypto),
        );

        service
            .revoke_factor("owner", user_id, factor_id, now)
            .await
            .expect("revoke factor");
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
        FactorError::WebAuthnCounterRegression => TwoFactorError::CodeReplayed,
        FactorError::Revoked => TwoFactorError::Revoked,
        FactorError::InvalidCodeFormat | FactorError::CodeMismatch => TwoFactorError::InvalidCode,
        FactorError::RecoveryMismatch | FactorError::RecoveryConsumed => {
            TwoFactorError::InvalidCode
        }
        FactorError::InvalidSecret => TwoFactorError::Storage("invalid secret".into()),
        FactorError::NotVerified => TwoFactorError::InvalidCode,
    }
}

// --- Remember-device cookie ---

impl TwoFactorService {
    /// Validate a remember-device cookie and confirm the bound factor
    /// is still active for the cookie's user. Returns `(user_id,
    /// factor_id)` on success.
    pub async fn accept_remember_device(
        &self,
        cookie: &str,
        user_agent: &str,
        ip: &str,
        now: DateTime<Utc>,
    ) -> Result<(Uuid, Uuid), TwoFactorError> {
        let payload = crate::identity::remember_device::validate_remember_device(
            &self.master_key,
            cookie,
            user_agent,
            ip,
            now,
        )?;
        // Confirm the user still owns an active factor with this id.
        let factors = self.factors.list_factors_for_user(payload.user_id).await?;
        let factor = factors
            .into_iter()
            .find(|f| f.id() == payload.factor_id && f.revoked_at().is_none())
            .ok_or(TwoFactorError::RememberDeviceMismatch)?;
        Ok((payload.user_id, factor.id()))
    }

    /// Issue a remember-device cookie for `factor_id` bound to the
    /// current request's UA and IP. Returns the cookie value.
    pub fn issue_remember_device_cookie(
        &self,
        user_id: Uuid,
        factor_id: Uuid,
        user_agent: &str,
        ip: &str,
        now: DateTime<Utc>,
    ) -> String {
        crate::identity::remember_device::issue_remember_device_with_lifetime(
            &self.master_key,
            user_id,
            factor_id,
            user_agent,
            ip,
            now,
            self.settings.remember_device_lifetime,
        )
    }
}
