//! SQLite-backed adapters for the domain repository traits.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    RepoError, SessionRepository, UserRepository,
    identity::{
        role::Role,
        session::{Session, SessionToken},
        user::User,
    },
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed adapter for the domain `UserRepository` trait.
#[derive(Clone)]
pub struct SqliteUserRepository {
    pool: Pool<Sqlite>,
}

impl SqliteUserRepository {
    /// Build a user repository over the given SQLite connection pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for SqliteUserRepository {
    async fn insert(&self, user: &User) -> Result<(), RepoError> {
        sqlx::query(
            r#"
            INSERT INTO users
                (id, username, email, password_hash, role, created_at, disabled_at, last_login_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(user.id().to_string())
        .bind(user.username().as_str())
        .bind(user.email().as_str())
        .bind(user.password().hash_str())
        .bind(user.role().as_str())
        .bind(user.created_at().to_rfc3339())
        .bind(user.disabled_at().map(|d| d.to_rfc3339()))
        .bind(user.last_login_at().map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, RepoError> {
        let row: Option<UserRow> = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, email, password_hash, role, created_at, disabled_at, last_login_at FROM users WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(UserRow::into_user).transpose()
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>, RepoError> {
        let row: Option<UserRow> = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, email, password_hash, role, created_at, disabled_at, last_login_at FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(UserRow::into_user).transpose()
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepoError> {
        let row: Option<UserRow> = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, email, password_hash, role, created_at, disabled_at, last_login_at FROM users WHERE email = ?",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(UserRow::into_user).transpose()
    }

    async fn list(&self) -> Result<Vec<User>, RepoError> {
        let rows: Vec<UserRow> = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, email, password_hash, role, created_at, disabled_at, last_login_at FROM users ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(UserRow::into_user).collect()
    }

    async fn update_role(&self, id: Uuid, role: Role) -> Result<(), RepoError> {
        sqlx::query("UPDATE users SET role = ? WHERE id = ?")
            .bind(role.as_str())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn disable(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("UPDATE users SET disabled_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn update_last_login(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn update_password(&self, id: Uuid, hash: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn count(&self) -> Result<i64, RepoError> {
        let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(n)
    }
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    username: String,
    email: String,
    password_hash: String,
    role: String,
    created_at: String,
    disabled_at: Option<String>,
    last_login_at: Option<String>,
}

impl UserRow {
    fn into_user(self) -> Result<User, RepoError> {
        let id =
            Uuid::parse_str(&self.id).map_err(|e| RepoError::new(format!("bad user id: {e}")))?;
        let username = openpanel_domain::Username::new(self.username)
            .map_err(|e| RepoError::new(e.to_string()))?;
        let email =
            openpanel_domain::Email::new(self.email).map_err(|e| RepoError::new(e.to_string()))?;
        let role: Role =
            self.role
                .parse()
                .map_err(|e: openpanel_domain::identity::role::RoleParseError| {
                    RepoError::new(e.to_string())
                })?;
        let created_at = parse_dt(&self.created_at)?;
        let disabled_at = self.disabled_at.as_deref().map(parse_dt).transpose()?;
        let last_login_at = self.last_login_at.as_deref().map(parse_dt).transpose()?;
        Ok(User::restore(
            id,
            username,
            email,
            self.password_hash,
            role,
            created_at,
            disabled_at,
            last_login_at,
        ))
    }
}

/// SQLite-backed adapter for the domain `SessionRepository` trait.
#[derive(Clone)]
pub struct SqliteSessionRepository {
    pool: Pool<Sqlite>,
}

impl SqliteSessionRepository {
    /// Build a session repository over the given SQLite connection pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn insert(&self, session: &Session) -> Result<(), RepoError> {
        sqlx::query(
            r#"
            INSERT INTO sessions
                (id, user_id, token_hash, role, created_at, last_seen_at, absolute_expires_at, source_ip, user_agent)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(session.id().to_string())
        .bind(session.user_id().to_string())
        .bind(&session.token_hash)
        .bind(session.role().as_str())
        .bind(session.created_at().to_rfc3339())
        .bind(session.last_seen_at().to_rfc3339())
        .bind(session.absolute_expires_at().to_rfc3339())
        .bind(session.source_ip())
        .bind(session.user_agent())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find_by_token_hash_match(
        &self,
        token: &SessionToken,
    ) -> Result<Option<Session>, RepoError> {
        // Token hashes use argon2id with random salt, so we can't index-by-hash.
        // We fetch all sessions and verify; this is acceptable for v0.1 panel
        // workloads (< 1000 active sessions).
        let rows: Vec<SessionRow> = sqlx::query_as::<_, SessionRow>(
            "SELECT id, user_id, token_hash, role, created_at, last_seen_at, absolute_expires_at, source_ip, user_agent FROM sessions",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        for row in rows {
            if let Ok(session) = row.into_session()
                && session.verify_token(token)
            {
                return Ok(Some(session));
            }
        }
        Ok(None)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Session>, RepoError> {
        let row: Option<SessionRow> = sqlx::query_as::<_, SessionRow>(
            "SELECT id, user_id, token_hash, role, created_at, last_seen_at, absolute_expires_at, source_ip, user_agent FROM sessions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(SessionRow::into_session).transpose()
    }

    async fn touch(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("UPDATE sessions SET last_seen_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn delete_for_user(&self, user_id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn purge_expired(&self) -> Result<u64, RepoError> {
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query("DELETE FROM sessions WHERE absolute_expires_at < ?")
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(res.rows_affected())
    }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    user_id: String,
    token_hash: String,
    role: String,
    created_at: String,
    last_seen_at: String,
    absolute_expires_at: String,
    source_ip: Option<String>,
    user_agent: Option<String>,
}

impl SessionRow {
    fn into_session(self) -> Result<Session, RepoError> {
        use openpanel_domain::identity::session::SessionBuilder;
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| RepoError::new(format!("bad session id: {e}")))?;
        let user_id = Uuid::parse_str(&self.user_id)
            .map_err(|e| RepoError::new(format!("bad user id: {e}")))?;
        let role: Role =
            self.role
                .parse()
                .map_err(|e: openpanel_domain::identity::role::RoleParseError| {
                    RepoError::new(e.to_string())
                })?;
        Ok(SessionBuilder {
            id,
            user_id,
            token_hash: self.token_hash,
            role,
            created_at: parse_dt(&self.created_at)?,
            last_seen_at: parse_dt(&self.last_seen_at)?,
            absolute_expires_at: parse_dt(&self.absolute_expires_at)?,
            source_ip: self.source_ip,
            user_agent: self.user_agent,
        }
        .build())
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("bad timestamp `{s}`: {e}")))
}
