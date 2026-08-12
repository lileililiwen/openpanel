//! SQLite repository adapter for two-factor authentication: factors,
//! encrypted TOTP secrets, recovery codes, pending-login challenges,
//! WebAuthn credentials, and in-flight WebAuthn ceremony state.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    RepoError,
    identity::{
        Factor, FactorKind, TotpEnrollmentChallenge, TwoFactorChallenge, UserVerificationPolicy,
        WebAuthnChallenge, WebAuthnCredential,
        factor::{FactorError, RecoveryCode},
        repository::FactorRepository,
    },
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// SQLite-backed implementation of [`FactorRepository`].
#[derive(Clone)]
pub struct SqliteFactorRepository {
    pool: Pool<Sqlite>,
}

impl SqliteFactorRepository {
    /// Construct a new repository over the given connection pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// The underlying pool (for the test harness).
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }
}

#[async_trait]
impl FactorRepository for SqliteFactorRepository {
    async fn insert_factor(
        &self,
        factor: &Factor,
        totp_secret_encrypted: &str,
    ) -> Result<(), RepoError> {
        let kind = factor.kind().as_str();
        let last_used_step = factor.last_used_step();
        sqlx::query(
            "INSERT INTO user_factors(
                id, user_id, kind, created_at, verified_at, last_used_at, revoked_at,
                last_used_step, totp_secret_encrypted
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(factor.id().to_string())
        .bind(factor.user_id().to_string())
        .bind(kind)
        .bind(factor.created_at().to_rfc3339())
        .bind(factor.verified_at().to_rfc3339())
        .bind(factor.last_used_at().map(|t| t.to_rfc3339()))
        .bind(factor.revoked_at().map(|t| t.to_rfc3339()))
        .bind(last_used_step)
        .bind(totp_secret_encrypted)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError(format!("insert_factor: {e}")))?;
        Ok(())
    }

    async fn find_factor(&self, id: Uuid) -> Result<Option<Factor>, RepoError> {
        let row = sqlx::query("SELECT * FROM user_factors WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepoError(format!("find_factor: {e}")))?;
        row.map(row_to_factor).transpose()
    }

    async fn update_factor(&self, factor: &Factor) -> Result<(), RepoError> {
        let last_used_step = factor.last_used_step();
        sqlx::query(
            "UPDATE user_factors SET
                last_used_at = ?, revoked_at = ?, last_used_step = ?
             WHERE id = ?",
        )
        .bind(factor.last_used_at().map(|t| t.to_rfc3339()))
        .bind(factor.revoked_at().map(|t| t.to_rfc3339()))
        .bind(last_used_step)
        .bind(factor.id().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError(format!("update_factor: {e}")))?;
        Ok(())
    }

    async fn list_factors_for_user(&self, user_id: Uuid) -> Result<Vec<Factor>, RepoError> {
        let rows = sqlx::query("SELECT * FROM user_factors WHERE user_id = ?")
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepoError(format!("list_factors_for_user: {e}")))?;
        rows.into_iter().map(row_to_factor).collect()
    }

    async fn find_active_totp_factor(&self, user_id: Uuid) -> Result<Option<Factor>, RepoError> {
        let row = sqlx::query(
            "SELECT * FROM user_factors
             WHERE user_id = ? AND kind = 'totp' AND revoked_at IS NULL",
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError(format!("find_active_totp_factor: {e}")))?;
        row.map(row_to_factor).transpose()
    }

    async fn find_totp_secret_encrypted(
        &self,
        factor_id: Uuid,
    ) -> Result<Option<String>, RepoError> {
        let row = sqlx::query("SELECT totp_secret_encrypted FROM user_factors WHERE id = ?")
            .bind(factor_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepoError(format!("find_totp_secret_encrypted: {e}")))?;
        Ok(row.and_then(|r| r.get::<Option<String>, _>("totp_secret_encrypted")))
    }

    async fn insert_totp_enrollment(
        &self,
        enrollment: &TotpEnrollmentChallenge,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO totp_enrollment_challenges(
                id, user_id, encrypted_secret, created_at, expires_at
             ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(enrollment.id().to_string())
        .bind(enrollment.user_id().to_string())
        .bind(enrollment.encrypted_secret())
        .bind(enrollment.created_at().to_rfc3339())
        .bind(enrollment.expires_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| RepoError(format!("insert_totp_enrollment: {error}")))?;
        Ok(())
    }

    async fn find_totp_enrollment(
        &self,
        id: Uuid,
    ) -> Result<Option<TotpEnrollmentChallenge>, RepoError> {
        let row = sqlx::query(
            "SELECT id, user_id, encrypted_secret, created_at, expires_at
             FROM totp_enrollment_challenges
             WHERE id = ? AND consumed_at IS NULL",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| RepoError(format!("find_totp_enrollment: {error}")))?;
        row.map(row_to_totp_enrollment).transpose()
    }

    async fn consume_totp_enrollment(
        &self,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<bool, RepoError> {
        let result = sqlx::query(
            "UPDATE totp_enrollment_challenges SET consumed_at = ?
             WHERE id = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| RepoError(format!("consume_totp_enrollment: {error}")))?;
        Ok(result.rows_affected() == 1)
    }

    async fn replace_recovery_codes(
        &self,
        user_id: Uuid,
        entries: &[(String, DateTime<Utc>)],
    ) -> Result<(), RepoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepoError(format!("replace_recovery_codes begin: {e}")))?;
        sqlx::query("DELETE FROM recovery_codes WHERE user_id = ?")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| RepoError(format!("replace_recovery_codes delete: {e}")))?;
        for (hash, created_at) in entries {
            sqlx::query(
                "INSERT INTO recovery_codes(user_id, code_hash, created_at) VALUES (?, ?, ?)",
            )
            .bind(user_id.to_string())
            .bind(hash)
            .bind(created_at.to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| RepoError(format!("replace_recovery_codes insert: {e}")))?;
        }
        tx.commit()
            .await
            .map_err(|e| RepoError(format!("replace_recovery_codes commit: {e}")))?;
        Ok(())
    }

    async fn list_recovery_codes(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(String, Option<DateTime<Utc>>)>, RepoError> {
        let rows =
            sqlx::query("SELECT code_hash, consumed_at FROM recovery_codes WHERE user_id = ?")
                .bind(user_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(|e| RepoError(format!("list_recovery_codes: {e}")))?;
        rows.into_iter()
            .map(|r| {
                let hash: String = r.get("code_hash");
                let consumed_at: Option<String> = r.get("consumed_at");
                let consumed_at = consumed_at
                    .map(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .map(|d| d.with_timezone(&Utc))
                            .map_err(|e| RepoError(format!("consumed_at parse: {e}")))
                    })
                    .transpose()?;
                Ok((hash, consumed_at))
            })
            .collect()
    }

    async fn consume_recovery_code(
        &self,
        user_id: Uuid,
        plaintext: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, RepoError> {
        let rows = sqlx::query(
            "SELECT id, code_hash FROM recovery_codes
             WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError(format!("consume_recovery_code select: {e}")))?;
        for row in rows {
            let id: i64 = row.get("id");
            let hash: String = row.get("code_hash");
            if RecoveryCode::verify(plaintext, &hash).is_ok() {
                sqlx::query("UPDATE recovery_codes SET consumed_at = ? WHERE id = ?")
                    .bind(now.to_rfc3339())
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| RepoError(format!("consume_recovery_code update: {e}")))?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn insert_challenge(&self, challenge: &TwoFactorChallenge) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO login_challenges(
                id, user_id, token_hash, created_at, expires_at, consumed_at,
                source_ip, user_agent
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(challenge.id().to_string())
        .bind(challenge.user_id().to_string())
        .bind(challenge.token_hash())
        .bind(challenge.created_at().to_rfc3339())
        .bind(challenge.expires_at().to_rfc3339())
        .bind(challenge.consumed_at().map(|t| t.to_rfc3339()))
        .bind(challenge.source_ip())
        .bind(challenge.user_agent())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError(format!("insert_challenge: {e}")))?;
        Ok(())
    }

    async fn find_challenge(&self, id: Uuid) -> Result<Option<TwoFactorChallenge>, RepoError> {
        let row = sqlx::query("SELECT * FROM login_challenges WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepoError(format!("find_challenge: {e}")))?;
        row.map(row_to_challenge).transpose()
    }

    async fn delete_challenge(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM login_challenges WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError(format!("delete_challenge: {e}")))?;
        Ok(())
    }

    async fn verify_challenge_token(
        &self,
        challenge_id: Uuid,
        plaintext: &str,
    ) -> Result<bool, RepoError> {
        let challenge = self.find_challenge(challenge_id).await?;
        Ok(challenge
            .map(|c| c.verify_token(plaintext))
            .unwrap_or(false))
    }

    async fn consume_challenge(
        &self,
        challenge_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<Uuid>, RepoError> {
        let challenge = self.find_challenge(challenge_id).await?;
        let Some(mut challenge) = challenge else {
            return Ok(None);
        };
        match challenge.consume(now) {
            Ok(user_id) => {
                sqlx::query("UPDATE login_challenges SET consumed_at = ? WHERE id = ?")
                    .bind(now.to_rfc3339())
                    .bind(challenge_id.to_string())
                    .execute(&self.pool)
                    .await
                    .map_err(|e| RepoError(format!("consume_challenge update: {e}")))?;
                Ok(Some(user_id))
            }
            Err(FactorError::CodeReplayed) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    async fn purge_expired_challenges(&self, now: DateTime<Utc>) -> Result<u64, RepoError> {
        let result = sqlx::query("DELETE FROM login_challenges WHERE expires_at < ?")
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError(format!("purge_expired_challenges: {e}")))?;
        Ok(result.rows_affected())
    }

    async fn insert_webauthn_credential<'a>(
        &self,
        credential_id: &str,
        user_id: Uuid,
        factor_id: Uuid,
        public_key_spki: &str,
        sign_count: i64,
        transports: Option<&'a str>,
        uv_policy: &str,
        passkey_json: &str,
        created_at: DateTime<Utc>,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO webauthn_credentials(
                id, user_id, factor_id, credential_id, public_key_spki,
                sign_count, transports, uv_policy, passkey_json, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(user_id.to_string())
        .bind(factor_id.to_string())
        .bind(credential_id)
        .bind(public_key_spki)
        .bind(sign_count)
        .bind(transports)
        .bind(uv_policy)
        .bind(passkey_json)
        .bind(created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError(format!("insert_webauthn_credential: {e}")))?;
        Ok(())
    }

    async fn list_webauthn_credentials(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<WebAuthnCredential>, RepoError> {
        let rows = sqlx::query(
            "SELECT * FROM webauthn_credentials
             WHERE user_id = ? AND revoked_at IS NULL
             ORDER BY created_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError(format!("list_webauthn_credentials: {e}")))?;
        rows.into_iter().map(row_to_webauthn_credential).collect()
    }

    async fn find_webauthn_credential_by_id(
        &self,
        credential_id: &str,
    ) -> Result<Option<WebAuthnCredential>, RepoError> {
        let row = sqlx::query("SELECT * FROM webauthn_credentials WHERE credential_id = ?")
            .bind(credential_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepoError(format!("find_webauthn_credential_by_id: {e}")))?;
        row.map(row_to_webauthn_credential).transpose()
    }

    async fn touch_webauthn_credential(
        &self,
        credential_id: &str,
        new_sign_count: i64,
        last_used_at: DateTime<Utc>,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE webauthn_credentials
             SET sign_count = ?, last_used_at = ?
             WHERE credential_id = ?",
        )
        .bind(new_sign_count)
        .bind(last_used_at.to_rfc3339())
        .bind(credential_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError(format!("touch_webauthn_credential: {e}")))?;
        Ok(())
    }

    async fn revoke_webauthn_credential(
        &self,
        credential_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), RepoError> {
        sqlx::query("UPDATE webauthn_credentials SET revoked_at = ? WHERE credential_id = ?")
            .bind(now.to_rfc3339())
            .bind(credential_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError(format!("revoke_webauthn_credential: {e}")))?;
        Ok(())
    }

    async fn insert_webauthn_challenge(
        &self,
        id: Uuid,
        user_id: Uuid,
        kind: &str,
        state_json: &str,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO webauthn_challenges(
                id, user_id, kind, state_json, created_at, expires_at
             ) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(kind)
        .bind(state_json)
        .bind(created_at.to_rfc3339())
        .bind(expires_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError(format!("insert_webauthn_challenge: {e}")))?;
        Ok(())
    }

    async fn find_webauthn_challenge(
        &self,
        id: Uuid,
    ) -> Result<Option<WebAuthnChallenge>, RepoError> {
        let row = sqlx::query("SELECT * FROM webauthn_challenges WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepoError(format!("find_webauthn_challenge: {e}")))?;
        row.map(row_to_webauthn_challenge).transpose()
    }

    async fn consume_webauthn_challenge(
        &self,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<WebAuthnChallenge>, RepoError> {
        let challenge = self.find_webauthn_challenge(id).await?;
        let Some(challenge) = challenge else {
            return Ok(None);
        };
        if !challenge.is_usable(now) {
            return Ok(None);
        }
        let result = sqlx::query(
            "UPDATE webauthn_challenges SET consumed_at = ?
             WHERE id = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError(format!("consume_webauthn_challenge: {e}")))?;
        if result.rows_affected() == 1 {
            Ok(Some(challenge))
        } else {
            Ok(None)
        }
    }
}

const CHALLENGE_TOKEN_HASH_COL: &str = "token_hash";

fn row_to_factor(row: sqlx::sqlite::SqliteRow) -> Result<Factor, RepoError> {
    let id: String = row.get("id");
    let user_id: String = row.get("user_id");
    let kind: String = row.get("kind");
    let created_at: String = row.get("created_at");
    let verified_at: String = row.get("verified_at");
    let last_used_at: Option<String> = row.get("last_used_at");
    let revoked_at: Option<String> = row.get("revoked_at");
    let last_used_step: Option<i64> = row.get("last_used_step");
    let id = Uuid::parse_str(&id).map_err(|e| RepoError(format!("factor id: {e}")))?;
    let user_id =
        Uuid::parse_str(&user_id).map_err(|e| RepoError(format!("factor user_id: {e}")))?;
    let kind = FactorKind::parse(&kind).ok_or_else(|| RepoError(format!("factor kind: {kind}")))?;
    let created_at = parse_dt(&created_at, "factor created_at")?;
    let verified_at = parse_dt(&verified_at, "factor verified_at")?;
    let last_used_at = last_used_at
        .map(|s| parse_dt(&s, "factor last_used_at"))
        .transpose()?;
    let revoked_at = revoked_at
        .map(|s| parse_dt(&s, "factor revoked_at"))
        .transpose()?;
    Ok(Factor::restore(
        id,
        user_id,
        kind,
        created_at,
        verified_at,
        last_used_at,
        revoked_at,
        last_used_step,
    ))
}

fn row_to_totp_enrollment(
    row: sqlx::sqlite::SqliteRow,
) -> Result<TotpEnrollmentChallenge, RepoError> {
    let id: String = row.get("id");
    let user_id: String = row.get("user_id");
    let encrypted_secret: String = row.get("encrypted_secret");
    let created_at: String = row.get("created_at");
    let expires_at: String = row.get("expires_at");
    Ok(TotpEnrollmentChallenge::restore(
        Uuid::parse_str(&id).map_err(|error| RepoError(format!("TOTP enrollment id: {error}")))?,
        Uuid::parse_str(&user_id)
            .map_err(|error| RepoError(format!("TOTP enrollment user: {error}")))?,
        encrypted_secret,
        parse_dt(&created_at, "TOTP enrollment created_at")?,
        parse_dt(&expires_at, "TOTP enrollment expires_at")?,
    ))
}

fn row_to_challenge(row: sqlx::sqlite::SqliteRow) -> Result<TwoFactorChallenge, RepoError> {
    let id: String = row.get("id");
    let user_id: String = row.get("user_id");
    let token_hash: String = row.get(CHALLENGE_TOKEN_HASH_COL);
    let created_at: String = row.get("created_at");
    let expires_at: String = row.get("expires_at");
    let consumed_at: Option<String> = row.get("consumed_at");
    let source_ip: Option<String> = row.get("source_ip");
    let user_agent: Option<String> = row.get("user_agent");
    let id = Uuid::parse_str(&id).map_err(|e| RepoError(format!("challenge id: {e}")))?;
    let user_id =
        Uuid::parse_str(&user_id).map_err(|e| RepoError(format!("challenge user_id: {e}")))?;
    let created_at = parse_dt(&created_at, "challenge created_at")?;
    let expires_at = parse_dt(&expires_at, "challenge expires_at")?;
    let consumed_at = consumed_at
        .map(|s| parse_dt(&s, "challenge consumed_at"))
        .transpose()?;
    Ok(TwoFactorChallenge::restore(
        id,
        user_id,
        token_hash,
        created_at,
        expires_at,
        consumed_at,
        source_ip,
        user_agent,
    ))
}

fn parse_dt(s: &str, label: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| RepoError(format!("{label}: {e}")))
}

fn row_to_webauthn_credential(
    row: sqlx::sqlite::SqliteRow,
) -> Result<WebAuthnCredential, RepoError> {
    let id: String = row.get("id");
    let user_id: String = row.get("user_id");
    let factor_id: String = row.get("factor_id");
    let credential_id: String = row.get("credential_id");
    let public_key_spki: String = row.get("public_key_spki");
    let sign_count: i64 = row.get("sign_count");
    let transports: Option<String> = row.get("transports");
    let uv_policy: String = row.get("uv_policy");
    let passkey_json: String = row.get("passkey_json");
    let created_at: String = row.get("created_at");
    let last_used_at: Option<String> = row.get("last_used_at");
    let revoked_at: Option<String> = row.get("revoked_at");
    let id = Uuid::parse_str(&id).map_err(|e| RepoError(format!("webauthn id: {e}")))?;
    let user_id =
        Uuid::parse_str(&user_id).map_err(|e| RepoError(format!("webauthn user_id: {e}")))?;
    let factor_id =
        Uuid::parse_str(&factor_id).map_err(|e| RepoError(format!("webauthn factor_id: {e}")))?;
    let uv = UserVerificationPolicy::parse(&uv_policy)
        .ok_or_else(|| RepoError(format!("webauthn uv_policy: {uv_policy}")))?;
    Ok(WebAuthnCredential::restore(
        id,
        user_id,
        factor_id,
        credential_id,
        public_key_spki,
        sign_count,
        transports.unwrap_or_default(),
        uv,
        passkey_json,
        parse_dt(&created_at, "webauthn created_at")?,
        last_used_at
            .map(|s| parse_dt(&s, "webauthn last_used_at"))
            .transpose()?,
        revoked_at
            .map(|s| parse_dt(&s, "webauthn revoked_at"))
            .transpose()?,
    ))
}

fn row_to_webauthn_challenge(row: sqlx::sqlite::SqliteRow) -> Result<WebAuthnChallenge, RepoError> {
    let id: String = row.get("id");
    let user_id: String = row.get("user_id");
    let kind: String = row.get("kind");
    let state_json: String = row.get("state_json");
    let created_at: String = row.get("created_at");
    let expires_at: String = row.get("expires_at");
    let consumed_at: Option<String> = row.get("consumed_at");
    let id = Uuid::parse_str(&id).map_err(|e| RepoError(format!("webauthn challenge id: {e}")))?;
    let user_id = Uuid::parse_str(&user_id)
        .map_err(|e| RepoError(format!("webauthn challenge user: {e}")))?;
    Ok(WebAuthnChallenge::restore(
        id,
        user_id,
        kind,
        state_json,
        parse_dt(&created_at, "webauthn challenge created_at")?,
        parse_dt(&expires_at, "webauthn challenge expires_at")?,
        consumed_at
            .map(|s| parse_dt(&s, "webauthn challenge consumed_at"))
            .transpose()?,
    ))
}
