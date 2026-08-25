//! SQLite SSO repository adapter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    RepoError, Role,
    identity::sso::{ExternalIdentity, SsoConnection, SsoLoginState, SsoRepository},
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed connection/identity/state store.
pub struct SqliteSsoRepository {
    pool: Pool<Sqlite>,
}

impl SqliteSsoRepository {
    /// Construct the adapter over a SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::User => "user",
    }
}

fn parse_role(value: &str) -> Role {
    match value {
        "owner" => Role::Owner,
        "admin" => Role::Admin,
        _ => Role::User,
    }
}

#[async_trait]
impl SsoRepository for SqliteSsoRepository {
    async fn get_connection(&self) -> Result<Option<SsoConnection>, RepoError> {
        let row = sqlx::query_as::<_, (String, String, String, String, bool, bool)>(
            "SELECT issuer_url, client_id, client_secret_cipher, default_role, \
             auto_provision, trust_idp_mfa FROM sso_connections WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(row.map(|tuple| {
            let (
                issuer_url,
                client_id,
                client_secret_cipher,
                default_role,
                auto_provision,
                trust_idp_mfa,
            ) = tuple;
            SsoConnection {
                issuer_url,
                client_id,
                client_secret_cipher,
                default_role: parse_role(&default_role),
                auto_provision,
                trust_idp_mfa,
            }
        }))
    }

    async fn save_connection(&self, connection: &SsoConnection) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO sso_connections (id, issuer_url, client_id, client_secret_cipher, \
             default_role, auto_provision, trust_idp_mfa) \
             VALUES (1, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET issuer_url = excluded.issuer_url, \
             client_id = excluded.client_id, \
             client_secret_cipher = excluded.client_secret_cipher, \
             default_role = excluded.default_role, \
             auto_provision = excluded.auto_provision, \
             trust_idp_mfa = excluded.trust_idp_mfa",
        )
        .bind(&connection.issuer_url)
        .bind(&connection.client_id)
        .bind(&connection.client_secret_cipher)
        .bind(role_str(connection.default_role))
        .bind(connection.auto_provision)
        .bind(connection.trust_idp_mfa)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<ExternalIdentity>, RepoError> {
        let row = sqlx::query_as::<_, (String, String, String)>(
            "SELECT issuer, subject, user_id FROM external_identities \
             WHERE issuer = ? AND subject = ?",
        )
        .bind(issuer)
        .bind(subject)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(|(issuer, subject, user_id)| {
            Uuid::parse_str(&user_id)
                .map(|parsed| ExternalIdentity {
                    issuer,
                    subject,
                    user_id: parsed,
                })
                .map_err(|e| RepoError::new(e.to_string()))
        })
        .transpose()
    }

    async fn save_identity(&self, identity: &ExternalIdentity) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO external_identities (issuer, subject, user_id) VALUES (?, ?, ?) \
             ON CONFLICT(issuer, subject) DO UPDATE SET user_id = excluded.user_id",
        )
        .bind(&identity.issuer)
        .bind(&identity.subject)
        .bind(identity.user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn insert_state(&self, state: &SsoLoginState) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO sso_states (state, nonce, pkce_verifier, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&state.state)
        .bind(&state.nonce)
        .bind(&state.pkce_verifier)
        .bind(state.expires_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn take_state(&self, state: &str) -> Result<Option<SsoLoginState>, RepoError> {
        let row = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT state, nonce, pkce_verifier, expires_at FROM sso_states WHERE state = ?",
        )
        .bind(state)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        // Single use: delete on read.
        sqlx::query("DELETE FROM sso_states WHERE state = ?")
            .bind(state)
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(|(state, nonce, pkce_verifier, expires_at)| {
            DateTime::parse_from_rfc3339(&expires_at)
                .map(|parsed| SsoLoginState {
                    state,
                    nonce,
                    pkce_verifier,
                    expires_at: parsed.with_timezone(&Utc),
                })
                .map_err(|e| RepoError::new(e.to_string()))
        })
        .transpose()
    }

    async fn purge_states(&self, cutoff: DateTime<Utc>) -> Result<u64, RepoError> {
        let result = sqlx::query("DELETE FROM sso_states WHERE expires_at < ?")
            .bind(cutoff.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(result.rows_affected())
    }
}
