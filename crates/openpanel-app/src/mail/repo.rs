//! Mail repository adapters.
use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;
use openpanel_domain::mail::{MailAddress, MailDomainName, MailQuota};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use super::{MailAlias, MailDomain, MailRepository, MailServiceError, StoredMailbox};

/// In-memory mail repository.
#[derive(Default)]
pub struct MemoryMailRepository {
    domains: Mutex<HashMap<Uuid, MailDomain>>,
    mailboxes: Mutex<HashMap<Uuid, StoredMailbox>>,
    aliases: Mutex<HashMap<Uuid, MailAlias>>,
}
#[async_trait]
impl MailRepository for MemoryMailRepository {
    async fn save_domain(&self, domain: &MailDomain) -> Result<(), MailServiceError> {
        self.domains
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .insert(domain.id, domain.clone());
        Ok(())
    }

    async fn domain(&self, id: Uuid) -> Result<MailDomain, MailServiceError> {
        self.domains
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .get(&id)
            .cloned()
            .ok_or(MailServiceError::NotFound)
    }

    async fn save_mailbox(&self, stored: &StoredMailbox) -> Result<(), MailServiceError> {
        self.mailboxes
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .insert(stored.mailbox.id, stored.clone());
        Ok(())
    }

    async fn mailboxes(&self, domain_id: Uuid) -> Result<Vec<StoredMailbox>, MailServiceError> {
        Ok(self
            .mailboxes
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .values()
            .filter(|stored| stored.mailbox.domain_id == domain_id)
            .cloned()
            .collect())
    }

    async fn domains(&self, owner: Option<Uuid>) -> Result<Vec<MailDomain>, MailServiceError> {
        Ok(self
            .domains
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .values()
            .filter(|domain| owner.is_none_or(|id| domain.owner_id == id))
            .cloned()
            .collect())
    }

    async fn domain_by_name(&self, name: &MailDomainName) -> Result<MailDomain, MailServiceError> {
        self.domains
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .values()
            .find(|domain| &domain.name == name)
            .cloned()
            .ok_or(MailServiceError::NotFound)
    }

    async fn mailbox_by_address(
        &self,
        address: &MailAddress,
    ) -> Result<StoredMailbox, MailServiceError> {
        self.mailboxes
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .values()
            .find(|stored| &stored.mailbox.address == address)
            .cloned()
            .ok_or(MailServiceError::NotFound)
    }

    async fn save_alias(&self, alias: &MailAlias) -> Result<(), MailServiceError> {
        self.aliases
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .insert(alias.id, alias.clone());
        Ok(())
    }

    async fn aliases(&self, domain_id: Uuid) -> Result<Vec<MailAlias>, MailServiceError> {
        Ok(self
            .aliases
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .values()
            .filter(|alias| alias.domain_id == domain_id)
            .cloned()
            .collect())
    }

    async fn delete_domain(&self, id: Uuid) -> Result<(), MailServiceError> {
        if self
            .domains
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .remove(&id)
            .is_none()
        {
            return Err(MailServiceError::NotFound);
        }
        self.mailboxes
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .retain(|_, item| item.mailbox.domain_id != id);
        self.aliases
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .retain(|_, item| item.domain_id != id);
        Ok(())
    }

    async fn delete_mailbox(&self, id: Uuid) -> Result<(), MailServiceError> {
        self.mailboxes
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .remove(&id)
            .map(|_| ())
            .ok_or(MailServiceError::NotFound)
    }

    async fn delete_alias(&self, id: Uuid) -> Result<(), MailServiceError> {
        self.aliases
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .remove(&id)
            .map(|_| ())
            .ok_or(MailServiceError::NotFound)
    }

    async fn alias(&self, id: Uuid) -> Result<MailAlias, MailServiceError> {
        self.aliases
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .get(&id)
            .cloned()
            .ok_or(MailServiceError::NotFound)
    }

    async fn counts(&self) -> Result<(usize, usize), MailServiceError> {
        Ok((
            self.domains
                .lock()
                .map_err(|_| MailServiceError::Repository)?
                .len(),
            self.mailboxes
                .lock()
                .map_err(|_| MailServiceError::Repository)?
                .len(),
        ))
    }
}

/// SQLite mail repository.
pub struct SqliteMailRepository {
    pool: Pool<Sqlite>,
}
impl SqliteMailRepository {
    /// Construct from application pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl MailRepository for SqliteMailRepository {
    async fn save_domain(&self, domain: &MailDomain) -> Result<(), MailServiceError> {
        sqlx::query("INSERT INTO mail_domains(id,owner_id,name,enabled,quota_bytes) VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET enabled=excluded.enabled,quota_bytes=excluded.quota_bytes").bind(domain.id.to_string()).bind(domain.owner_id.to_string()).bind(domain.name.as_str()).bind(domain.enabled).bind(i64::try_from(domain.quota_bytes).map_err(|_|MailServiceError::Repository)?).execute(&self.pool).await.map_err(|_|MailServiceError::Repository)?;
        Ok(())
    }

    async fn domain(&self, id: Uuid) -> Result<MailDomain, MailServiceError> {
        let row =
            sqlx::query("SELECT id,owner_id,name,enabled,quota_bytes FROM mail_domains WHERE id=?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| MailServiceError::Repository)?
                .ok_or(MailServiceError::NotFound)?;
        domain_row(&row)
    }

    async fn save_mailbox(&self, stored: &StoredMailbox) -> Result<(), MailServiceError> {
        sqlx::query("INSERT INTO mail_mailboxes(id,domain_id,address,quota_bytes,enabled,password_hash) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET quota_bytes=excluded.quota_bytes,enabled=excluded.enabled,password_hash=excluded.password_hash").bind(stored.mailbox.id.to_string()).bind(stored.mailbox.domain_id.to_string()).bind(stored.mailbox.address.as_str()).bind(i64::try_from(stored.mailbox.quota.bytes()).map_err(|_|MailServiceError::Repository)?).bind(stored.mailbox.enabled).bind(&stored.password_hash).execute(&self.pool).await.map_err(|_|MailServiceError::Repository)?;
        Ok(())
    }

    async fn mailboxes(&self, domain_id: Uuid) -> Result<Vec<StoredMailbox>, MailServiceError> {
        let rows=sqlx::query("SELECT id,domain_id,address,quota_bytes,enabled,password_hash FROM mail_mailboxes WHERE domain_id=? ORDER BY address").bind(domain_id.to_string()).fetch_all(&self.pool).await.map_err(|_|MailServiceError::Repository)?;
        rows.iter().map(mailbox_row).collect()
    }

    async fn domains(&self, owner: Option<Uuid>) -> Result<Vec<MailDomain>, MailServiceError> {
        let rows=if let Some(owner)=owner{sqlx::query("SELECT id,owner_id,name,enabled,quota_bytes FROM mail_domains WHERE owner_id=? ORDER BY name").bind(owner.to_string()).fetch_all(&self.pool).await}else{sqlx::query("SELECT id,owner_id,name,enabled,quota_bytes FROM mail_domains ORDER BY name").fetch_all(&self.pool).await}.map_err(|_|MailServiceError::Repository)?;
        rows.iter().map(domain_row).collect()
    }

    async fn domain_by_name(&self, name: &MailDomainName) -> Result<MailDomain, MailServiceError> {
        let row = sqlx::query(
            "SELECT id,owner_id,name,enabled,quota_bytes FROM mail_domains WHERE name=?",
        )
        .bind(name.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| MailServiceError::Repository)?
        .ok_or(MailServiceError::NotFound)?;
        domain_row(&row)
    }

    async fn mailbox_by_address(
        &self,
        address: &MailAddress,
    ) -> Result<StoredMailbox, MailServiceError> {
        let row=sqlx::query("SELECT id,domain_id,address,quota_bytes,enabled,password_hash FROM mail_mailboxes WHERE address=?").bind(address.as_str()).fetch_optional(&self.pool).await.map_err(|_|MailServiceError::Repository)?.ok_or(MailServiceError::NotFound)?;
        mailbox_row(&row)
    }

    async fn save_alias(&self, alias: &MailAlias) -> Result<(), MailServiceError> {
        sqlx::query("INSERT INTO mail_aliases(id,domain_id,source,destination) VALUES(?,?,?,?)")
            .bind(alias.id.to_string())
            .bind(alias.domain_id.to_string())
            .bind(alias.source.as_str())
            .bind(alias.destination.as_str())
            .execute(&self.pool)
            .await
            .map_err(|_| MailServiceError::Repository)?;
        Ok(())
    }

    async fn aliases(&self, domain_id: Uuid) -> Result<Vec<MailAlias>, MailServiceError> {
        let rows = sqlx::query(
            "SELECT id,domain_id,source,destination FROM mail_aliases WHERE domain_id=?",
        )
        .bind(domain_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| MailServiceError::Repository)?;
        rows.iter().map(alias_row).collect()
    }

    async fn delete_domain(&self, id: Uuid) -> Result<(), MailServiceError> {
        let result = sqlx::query("DELETE FROM mail_domains WHERE id=?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|_| MailServiceError::Repository)?;
        if result.rows_affected() == 0 {
            return Err(MailServiceError::NotFound);
        }
        Ok(())
    }

    async fn delete_mailbox(&self, id: Uuid) -> Result<(), MailServiceError> {
        let result = sqlx::query("DELETE FROM mail_mailboxes WHERE id=?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|_| MailServiceError::Repository)?;
        (result.rows_affected() == 1)
            .then_some(())
            .ok_or(MailServiceError::NotFound)
    }

    async fn delete_alias(&self, id: Uuid) -> Result<(), MailServiceError> {
        let result = sqlx::query("DELETE FROM mail_aliases WHERE id=?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|_| MailServiceError::Repository)?;
        (result.rows_affected() == 1)
            .then_some(())
            .ok_or(MailServiceError::NotFound)
    }

    async fn alias(&self, id: Uuid) -> Result<MailAlias, MailServiceError> {
        let row =
            sqlx::query("SELECT id,domain_id,source,destination FROM mail_aliases WHERE id=?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| MailServiceError::Repository)?
                .ok_or(MailServiceError::NotFound)?;
        alias_row(&row)
    }

    async fn counts(&self) -> Result<(usize, usize), MailServiceError> {
        let d: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mail_domains")
            .fetch_one(&self.pool)
            .await
            .map_err(|_| MailServiceError::Repository)?;
        let m: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mail_mailboxes")
            .fetch_one(&self.pool)
            .await
            .map_err(|_| MailServiceError::Repository)?;
        Ok((
            usize::try_from(d).map_err(|_| MailServiceError::Repository)?,
            usize::try_from(m).map_err(|_| MailServiceError::Repository)?,
        ))
    }
}
fn domain_row(row: &sqlx::sqlite::SqliteRow) -> Result<MailDomain, MailServiceError> {
    let quota: i64 = row
        .try_get("quota_bytes")
        .map_err(|_| MailServiceError::Repository)?;
    Ok(MailDomain {
        id: Uuid::parse_str(
            row.try_get("id")
                .map_err(|_| MailServiceError::Repository)?,
        )
        .map_err(|_| MailServiceError::Repository)?,
        owner_id: Uuid::parse_str(
            row.try_get("owner_id")
                .map_err(|_| MailServiceError::Repository)?,
        )
        .map_err(|_| MailServiceError::Repository)?,
        name: MailDomainName::new(
            row.try_get::<String, _>("name")
                .map_err(|_| MailServiceError::Repository)?,
        )
        .map_err(|_| MailServiceError::Repository)?,
        enabled: row
            .try_get("enabled")
            .map_err(|_| MailServiceError::Repository)?,
        quota_bytes: u64::try_from(quota).map_err(|_| MailServiceError::Repository)?,
    })
}
fn mailbox_row(row: &sqlx::sqlite::SqliteRow) -> Result<StoredMailbox, MailServiceError> {
    let quota: i64 = row
        .try_get("quota_bytes")
        .map_err(|_| MailServiceError::Repository)?;
    Ok(StoredMailbox {
        mailbox: super::Mailbox {
            id: Uuid::parse_str(
                row.try_get("id")
                    .map_err(|_| MailServiceError::Repository)?,
            )
            .map_err(|_| MailServiceError::Repository)?,
            domain_id: Uuid::parse_str(
                row.try_get("domain_id")
                    .map_err(|_| MailServiceError::Repository)?,
            )
            .map_err(|_| MailServiceError::Repository)?,
            address: MailAddress::parse(
                row.try_get("address")
                    .map_err(|_| MailServiceError::Repository)?,
            )
            .map_err(|_| MailServiceError::Repository)?,
            quota: MailQuota::new(
                u64::try_from(quota).map_err(|_| MailServiceError::Repository)?,
                1,
                u64::MAX,
            )
            .map_err(|_| MailServiceError::Repository)?,
            enabled: row
                .try_get("enabled")
                .map_err(|_| MailServiceError::Repository)?,
        },
        password_hash: row
            .try_get("password_hash")
            .map_err(|_| MailServiceError::Repository)?,
    })
}
fn alias_row(row: &sqlx::sqlite::SqliteRow) -> Result<MailAlias, MailServiceError> {
    Ok(MailAlias {
        id: Uuid::parse_str(
            row.try_get("id")
                .map_err(|_| MailServiceError::Repository)?,
        )
        .map_err(|_| MailServiceError::Repository)?,
        domain_id: Uuid::parse_str(
            row.try_get("domain_id")
                .map_err(|_| MailServiceError::Repository)?,
        )
        .map_err(|_| MailServiceError::Repository)?,
        source: MailAddress::parse(
            row.try_get("source")
                .map_err(|_| MailServiceError::Repository)?,
        )
        .map_err(|_| MailServiceError::Repository)?,
        destination: MailAddress::parse(
            row.try_get("destination")
                .map_err(|_| MailServiceError::Repository)?,
        )
        .map_err(|_| MailServiceError::Repository)?,
    })
}
