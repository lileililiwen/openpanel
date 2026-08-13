//! SQLite Docker desired-state repository.

use async_trait::async_trait;
use openpanel_domain::{
    RepoError,
    docker::{Container, DockerRepository, ImageAllowlistEntry, StoredComposeStack},
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::databases::crypto;

/// SQLite-backed container and allowlist persistence.
pub struct SqliteDockerRepository {
    pool: SqlitePool,
    master_key: [u8; 32],
}

impl SqliteDockerRepository {
    /// Construct over an application pool.
    pub fn new(pool: SqlitePool, master_key: [u8; 32]) -> Self {
        Self { pool, master_key }
    }
}

#[async_trait]
impl DockerRepository for SqliteDockerRepository {
    async fn put_container(&self, container: &Container) -> Result<(), RepoError> {
        let mut stored_spec = container.spec.clone();
        let env_json = serde_json::to_string(&stored_spec.env)
            .map_err(|error| RepoError::new(error.to_string()))?;
        stored_spec.env.clear();
        let spec = serde_json::to_string(&stored_spec)
            .map_err(|error| RepoError::new(error.to_string()))?;
        let env_cipher = crypto::encrypt_to_storage(&self.master_key, &env_json)
            .map_err(|error| RepoError::new(error.to_string()))?;
        sqlx::query("INSERT INTO docker_containers (id,spec_json,env_cipher,runtime_id,status,oom_killed,created_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET spec_json=excluded.spec_json,env_cipher=excluded.env_cipher,runtime_id=excluded.runtime_id,status=excluded.status,oom_killed=excluded.oom_killed")
            .bind(container.spec.id.to_string())
            .bind(spec)
            .bind(env_cipher)
            .bind(&container.runtime_id)
            .bind(&container.status)
            .bind(container.oom_killed)
            .bind(container.created_at.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn get_container(&self, id: Uuid) -> Result<Option<Container>, RepoError> {
        sqlx::query("SELECT spec_json,env_cipher,runtime_id,status,oom_killed,created_at FROM docker_containers WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .map(|row| self.decode_container(row))
            .transpose()
    }

    async fn list_containers(&self) -> Result<Vec<Container>, RepoError> {
        sqlx::query("SELECT spec_json,env_cipher,runtime_id,status,oom_killed,created_at FROM docker_containers ORDER BY created_at,id")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .into_iter()
            .map(|row| self.decode_container(row))
            .collect()
    }

    async fn delete_container(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM docker_containers WHERE id=?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn allowlist(&self) -> Result<Vec<ImageAllowlistEntry>, RepoError> {
        sqlx::query("SELECT pattern,allow_pull,pin_digest_required FROM docker_image_allowlist ORDER BY pattern")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .into_iter()
            .map(|row| {
                Ok(ImageAllowlistEntry {
                    pattern: row.try_get("pattern").map_err(db)?,
                    allow_pull: row.try_get("allow_pull").map_err(db)?,
                    pin_digest_required: row.try_get("pin_digest_required").map_err(db)?,
                })
            })
            .collect()
    }

    async fn put_allowlist(&self, entry: &ImageAllowlistEntry) -> Result<(), RepoError> {
        sqlx::query("INSERT INTO docker_image_allowlist(pattern,allow_pull,pin_digest_required) VALUES(?,?,?) ON CONFLICT(pattern) DO UPDATE SET allow_pull=excluded.allow_pull,pin_digest_required=excluded.pin_digest_required")
            .bind(&entry.pattern)
            .bind(entry.allow_pull)
            .bind(entry.pin_digest_required)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn put_stack(&self, stack: &StoredComposeStack) -> Result<(), RepoError> {
        let json = serde_json::to_string(&stack.stack)
            .map_err(|error| RepoError::new(error.to_string()))?;
        let cipher = crypto::encrypt_to_storage(&self.master_key, &json)
            .map_err(|error| RepoError::new(error.to_string()))?;
        sqlx::query("INSERT INTO docker_stacks(id,name,stack_cipher,status,last_applied_at) VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,stack_cipher=excluded.stack_cipher,status=excluded.status,last_applied_at=excluded.last_applied_at")
            .bind(stack.stack.id.to_string())
            .bind(&stack.stack.name)
            .bind(cipher)
            .bind(&stack.status)
            .bind(stack.last_applied_at.map(|value| value.to_rfc3339()))
            .execute(&self.pool).await.map_err(db)?;
        Ok(())
    }

    async fn get_stack(&self, id: Uuid) -> Result<Option<StoredComposeStack>, RepoError> {
        sqlx::query("SELECT stack_cipher,status,last_applied_at FROM docker_stacks WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .map(|row| self.decode_stack(row))
            .transpose()
    }

    async fn list_stacks(&self) -> Result<Vec<StoredComposeStack>, RepoError> {
        sqlx::query("SELECT stack_cipher,status,last_applied_at FROM docker_stacks ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .into_iter()
            .map(|row| self.decode_stack(row))
            .collect()
    }

    async fn delete_stack(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM docker_stacks WHERE id=?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
}

impl SqliteDockerRepository {
    fn decode_container(&self, row: sqlx::sqlite::SqliteRow) -> Result<Container, RepoError> {
        let mut spec: openpanel_domain::docker::ContainerSpec =
            serde_json::from_str(row.try_get("spec_json").map_err(db)?)
                .map_err(|error| RepoError::new(error.to_string()))?;
        let env_cipher: String = row.try_get("env_cipher").map_err(db)?;
        let env_json = crypto::decrypt_from_storage(&self.master_key, &env_cipher)
            .map_err(|error| RepoError::new(error.to_string()))?;
        spec.env =
            serde_json::from_str(&env_json).map_err(|error| RepoError::new(error.to_string()))?;
        Ok(Container {
            spec,
            runtime_id: row.try_get("runtime_id").map_err(db)?,
            status: row.try_get("status").map_err(db)?,
            oom_killed: row.try_get("oom_killed").map_err(db)?,
            created_at: chrono::DateTime::parse_from_rfc3339(
                row.try_get::<String, _>("created_at").map_err(db)?.as_str(),
            )
            .map_err(|error| RepoError::new(error.to_string()))?
            .with_timezone(&chrono::Utc),
        })
    }

    fn decode_stack(&self, row: sqlx::sqlite::SqliteRow) -> Result<StoredComposeStack, RepoError> {
        let cipher: String = row.try_get("stack_cipher").map_err(db)?;
        let json = crypto::decrypt_from_storage(&self.master_key, &cipher)
            .map_err(|error| RepoError::new(error.to_string()))?;
        let last_applied_at = row
            .try_get::<Option<String>, _>("last_applied_at")
            .map_err(db)?
            .map(|value| {
                chrono::DateTime::parse_from_rfc3339(&value)
                    .map(|date| date.with_timezone(&chrono::Utc))
                    .map_err(|error| RepoError::new(error.to_string()))
            })
            .transpose()?;
        Ok(StoredComposeStack {
            stack: serde_json::from_str(&json)
                .map_err(|error| RepoError::new(error.to_string()))?,
            status: row.try_get("status").map_err(db)?,
            last_applied_at,
        })
    }
}

fn db(error: sqlx::Error) -> RepoError {
    RepoError::new(error.to_string())
}
