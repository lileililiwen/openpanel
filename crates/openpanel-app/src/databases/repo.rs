//! SQLite-backed adapter for `DatabaseRepository`.

use async_trait::async_trait;
use chrono::DateTime;
use openpanel_domain::{
    DatabaseRepository, RepoError,
    databases::{database::Database, engine::DatabaseEngine, status::DatabaseStatus},
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed adapter for the domain `DatabaseRepository` trait.
#[derive(Clone)]
pub struct SqliteDatabaseRepository {
    pool: Pool<Sqlite>,
}

impl SqliteDatabaseRepository {
    /// Build a repository over the given SQLite connection pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DatabaseRepository for SqliteDatabaseRepository {
    async fn insert(&self, db: &Database, password_ciphertext: &str) -> Result<(), RepoError> {
        sqlx::query(
            r#"
            INSERT INTO databases
                (id, owner_id, name, db_user, db_host, engine, charset,
                 status, password_ciphertext, created_at, created_by)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(db.id().to_string())
        .bind(db.owner_id().to_string())
        .bind(db.name())
        .bind(db.db_user())
        .bind(db.db_host())
        .bind(db.engine().as_str())
        .bind(db.charset())
        .bind(db.status().as_str())
        .bind(password_ciphertext)
        .bind(db.created_at().to_rfc3339())
        .bind(db.created_by())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Database>, RepoError> {
        let row: Option<DatabaseRow> = sqlx::query_as::<_, DatabaseRow>(
            "SELECT id, owner_id, name, db_user, db_host, engine, charset,
                    status, created_at, created_by FROM databases WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(DatabaseRow::into_database).transpose()
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Database>, RepoError> {
        let row: Option<DatabaseRow> = sqlx::query_as::<_, DatabaseRow>(
            "SELECT id, owner_id, name, db_user, db_host, engine, charset,
                    status, created_at, created_by FROM databases WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(DatabaseRow::into_database).transpose()
    }

    async fn list_all(&self) -> Result<Vec<Database>, RepoError> {
        let rows: Vec<DatabaseRow> = sqlx::query_as::<_, DatabaseRow>(
            "SELECT id, owner_id, name, db_user, db_host, engine, charset,
                    status, created_at, created_by FROM databases ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(DatabaseRow::into_database).collect()
    }

    async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<Database>, RepoError> {
        let rows: Vec<DatabaseRow> = sqlx::query_as::<_, DatabaseRow>(
            "SELECT id, owner_id, name, db_user, db_host, engine, charset,
                    status, created_at, created_by FROM databases WHERE owner_id = ? ORDER BY name",
        )
        .bind(owner_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(DatabaseRow::into_database).collect()
    }

    async fn password_ciphertext(&self, id: Uuid) -> Result<Option<String>, RepoError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT password_ciphertext FROM databases WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(row.map(|r| r.0))
    }

    async fn update_password_ciphertext(
        &self,
        id: Uuid,
        ciphertext: &str,
    ) -> Result<(), RepoError> {
        sqlx::query("UPDATE databases SET password_ciphertext = ? WHERE id = ?")
            .bind(ciphertext)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn update_status(&self, id: Uuid, status: DatabaseStatus) -> Result<(), RepoError> {
        sqlx::query("UPDATE databases SET status = ? WHERE id = ?")
            .bind(status.as_str())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM databases WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn count(&self) -> Result<i64, RepoError> {
        let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM databases")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(n)
    }
}

#[derive(sqlx::FromRow)]
struct DatabaseRow {
    id: String,
    owner_id: String,
    name: String,
    db_user: String,
    db_host: String,
    engine: String,
    charset: String,
    status: String,
    created_at: String,
    created_by: String,
}

impl DatabaseRow {
    fn into_database(self) -> Result<Database, RepoError> {
        let id =
            Uuid::parse_str(&self.id).map_err(|e| RepoError::new(format!("bad db id: {e}")))?;
        let owner_id = Uuid::parse_str(&self.owner_id)
            .map_err(|e| RepoError::new(format!("bad owner id: {e}")))?;
        let engine: DatabaseEngine = self
            .engine
            .parse()
            .map_err(|e: String| RepoError::new(format!("bad engine: {e}")))?;
        let status: DatabaseStatus = self
            .status
            .parse()
            .map_err(|e: String| RepoError::new(format!("bad status: {e}")))?;
        let created_at = DateTime::parse_from_rfc3339(&self.created_at)
            .map(|d| d.with_timezone(&chrono::Utc))
            .map_err(|e| RepoError::new(format!("bad timestamp: {e}")))?;
        Ok(Database::restore(
            id,
            owner_id,
            self.name,
            self.db_user,
            self.db_host,
            engine,
            self.charset,
            status,
            created_at,
            self.created_by,
        ))
    }
}
