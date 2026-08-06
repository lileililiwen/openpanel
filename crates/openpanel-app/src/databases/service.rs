//! Databases application service: orchestrates repository + MySQL CLI
//! shell-out + AES-256-GCM encryption + audit log + RBAC.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::databases::database::Database;
use openpanel_domain::databases::error::DatabaseError;
use openpanel_domain::{DatabaseRepository, Role, User};
use rand::Rng;
use uuid::Uuid;

use crate::databases::crypto;
use crate::databases::mysql::MySqlClient;

pub struct DatabasesService {
    repos: Arc<dyn DatabaseRepository>,
    mysql: MySqlClient,
    audit: Arc<dyn AuditService>,
    master_key: [u8; 32],
}

impl DatabasesService {
    pub fn new(
        repos: Arc<dyn DatabaseRepository>,
        mysql: MySqlClient,
        audit: Arc<dyn AuditService>,
        master_key: [u8; 32],
    ) -> Self {
        Self {
            repos,
            mysql,
            audit,
            master_key,
        }
    }

    pub fn mysql_available(&self) -> bool {
        self.mysql.available()
    }

    pub async fn create_database(
        &self,
        caller: &User,
        owner_id: Uuid,
        owner_username: &str,
        suffix: &str,
        charset: Option<String>,
    ) -> Result<(Database, String), DatabaseError> {
        if !caller.role().can_manage_databases() {
            return Err(DatabaseError::Forbidden);
        }

        let charset = charset.unwrap_or_else(|| "utf8mb4".to_string());
        let db = Database::new(
            Uuid::new_v4(),
            owner_id,
            owner_username,
            suffix,
            &charset,
            caller.username().as_str(),
        )?;

        if let Some(_existing) = self
            .repos
            .find_by_name(db.name())
            .await
            .map_err(|e| DatabaseError::Persistence(e.0))?
        {
            return Err(DatabaseError::DuplicateDatabase(db.name().to_string()));
        }

        let password = generate_password(24);

        // Provision MySQL first; if it fails, no DB row is created.
        self.mysql
            .create_database(db.name(), db.charset())
            .await?;
        self.mysql
            .create_user(db.db_user(), db.db_host(), &password)
            .await?;
        self.mysql
            .grant_all(db.name(), db.db_user(), db.db_host())
            .await?;

        let ciphertext = crypto::encrypt_to_storage(&self.master_key, &password)
            .map_err(|e| DatabaseError::Encryption(e.to_string()))?;

        if let Err(e) = self.repos.insert(&db, &ciphertext).await {
            // Best-effort rollback of MySQL state.
            let _ = self.mysql.drop_user(db.db_user(), db.db_host()).await;
            let _ = self.mysql.drop_database(db.name()).await;
            return Err(DatabaseError::Persistence(e.0));
        }

        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DatabaseCreated,
                    AuditOutcome::Success,
                )
                .target(db.id().to_string())
                .metadata(serde_json::json!({
                    "name": db.name(),
                    "engine": db.engine().as_str(),
                })),
            )
            .await
            .ok();
        Ok((db, password))
    }

    pub async fn list_databases(&self, caller: &User) -> Result<Vec<Database>, DatabaseError> {
        match caller.role() {
            Role::Owner => self
                .repos
                .list_all()
                .await
                .map_err(|e| DatabaseError::Persistence(e.0)),
            _ => self
                .repos
                .list_by_owner(caller.id())
                .await
                .map_err(|e| DatabaseError::Persistence(e.0)),
        }
    }

    pub async fn get_database(&self, caller: &User, id: Uuid) -> Result<Database, DatabaseError> {
        let db = self
            .repos
            .find_by_id(id)
            .await
            .map_err(|e| DatabaseError::Persistence(e.0))?
            .ok_or_else(|| DatabaseError::NotFound(id.to_string()))?;
        self.assert_can_view(caller, &db)?;
        Ok(db)
    }

    pub async fn delete_database(&self, caller: &User, id: Uuid) -> Result<(), DatabaseError> {
        let db = self.get_database(caller, id).await?;
        self.mysql.drop_database(db.name()).await?;
        self.mysql.drop_user(db.db_user(), db.db_host()).await?;
        self.repos
            .delete(id)
            .await
            .map_err(|e| DatabaseError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DatabaseDeleted,
                    AuditOutcome::Success,
                )
                .target(id.to_string())
                .metadata(serde_json::json!({"name": db.name()})),
            )
            .await
            .ok();
        Ok(())
    }

    pub async fn change_password(
        &self,
        caller: &User,
        id: Uuid,
    ) -> Result<String, DatabaseError> {
        let db = self.get_database(caller, id).await?;
        let new_password = generate_password(24);
        self.mysql
            .change_password(db.db_user(), db.db_host(), &new_password)
            .await?;
        let ciphertext = crypto::encrypt_to_storage(&self.master_key, &new_password)
            .map_err(|e| DatabaseError::Encryption(e.to_string()))?;
        self.repos
            .update_password_ciphertext(id, &ciphertext)
            .await
            .map_err(|e| DatabaseError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DatabasePasswordChanged,
                    AuditOutcome::Success,
                )
                .target(id.to_string())
                .metadata(serde_json::json!({"rotated": true})),
            )
            .await
            .ok();
        Ok(new_password)
    }

    pub async fn reveal_password(
        &self,
        caller: &User,
        id: Uuid,
    ) -> Result<String, DatabaseError> {
        let db = self.get_database(caller, id).await?;
        let stored = self
            .repos
            .password_ciphertext(id)
            .await
            .map_err(|e| DatabaseError::Persistence(e.0))?
            .ok_or_else(|| DatabaseError::NotFound(id.to_string()))?;
        let plaintext = crypto::decrypt_from_storage(&self.master_key, &stored)
            .map_err(|e| DatabaseError::Decryption(e.to_string()))?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::PermissionDenied,
                    AuditOutcome::Success,
                )
                .target(id.to_string())
                .metadata(serde_json::json!({"action": "reveal_password"})),
            )
            .await
            .ok();
        let _ = db;
        Ok(plaintext)
    }

    fn assert_can_view(&self, caller: &User, db: &Database) -> Result<(), DatabaseError> {
        match caller.role() {
            Role::Owner => Ok(()),
            Role::Admin => {
                if db.owner_id() == caller.id() {
                    Ok(())
                } else {
                    Ok(())
                }
            }
            Role::User => {
                if db.owner_id() == caller.id() {
                    Ok(())
                } else {
                    Err(DatabaseError::Forbidden)
                }
            }
        }
    }
}

fn generate_password(len: usize) -> String {
    const CHARS: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..CHARS.len());
            CHARS[idx] as char
        })
        .collect()
}