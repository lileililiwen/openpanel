//! DNS repository adapters.

use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use super::{
    DnsRepository, DnsServiceError, ProviderAccount, ProviderZone, RemoteRecord,
    StoredProviderAccount,
};

/// Deterministic in-memory repository used by tests and local fake-provider CLI workflows.
#[derive(Default)]
pub struct MemoryDnsRepository {
    accounts: Mutex<HashMap<Uuid, StoredProviderAccount>>,
    zones: Mutex<HashMap<Uuid, (Uuid, ProviderZone)>>,
    records: Mutex<HashMap<Uuid, Vec<RemoteRecord>>>,
}
#[async_trait]
impl DnsRepository for MemoryDnsRepository {
    async fn save_account(&self, account: &StoredProviderAccount) -> Result<(), DnsServiceError> {
        self.accounts
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .insert(account.account.id, account.clone());
        Ok(())
    }

    async fn account(&self, id: Uuid) -> Result<StoredProviderAccount, DnsServiceError> {
        self.accounts
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .get(&id)
            .cloned()
            .ok_or(DnsServiceError::NotFound)
    }

    async fn save_zone(
        &self,
        account_id: Uuid,
        zone: &ProviderZone,
    ) -> Result<(), DnsServiceError> {
        self.zones
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .insert(zone.id, (account_id, zone.clone()));
        Ok(())
    }

    async fn import_records(
        &self,
        zone_id: Uuid,
        records: &[RemoteRecord],
    ) -> Result<(), DnsServiceError> {
        let mut all = self
            .records
            .lock()
            .map_err(|_| DnsServiceError::Repository)?;
        let local = all.entry(zone_id).or_default();
        for record in records {
            if let Some(existing) = local
                .iter_mut()
                .find(|item| item.remote_id == record.remote_id)
            {
                *existing = record.clone();
            } else {
                local.push(record.clone());
            }
        }
        Ok(())
    }

    async fn local_records(&self, zone_id: Uuid) -> Result<Vec<RemoteRecord>, DnsServiceError> {
        Ok(self
            .records
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .get(&zone_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn accounts(&self) -> Result<Vec<ProviderAccount>, DnsServiceError> {
        Ok(self
            .accounts
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .values()
            .map(|stored| stored.account.clone())
            .collect())
    }

    async fn zones(&self) -> Result<Vec<ProviderZone>, DnsServiceError> {
        Ok(self
            .zones
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .values()
            .map(|(_, zone)| zone.clone())
            .collect())
    }

    async fn zone_account(
        &self,
        zone_id: Uuid,
    ) -> Result<(ProviderZone, StoredProviderAccount), DnsServiceError> {
        let (account_id, zone) = self
            .zones
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .get(&zone_id)
            .cloned()
            .ok_or(DnsServiceError::NotFound)?;
        Ok((zone, self.account(account_id).await?))
    }

    async fn delete_local_record(
        &self,
        zone_id: Uuid,
        remote_id: &str,
    ) -> Result<(), DnsServiceError> {
        if let Some(records) = self
            .records
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .get_mut(&zone_id)
        {
            records.retain(|record| record.remote_id != remote_id);
        }
        Ok(())
    }

    async fn delete_account(&self, id: Uuid) -> Result<(), DnsServiceError> {
        if self
            .accounts
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .remove(&id)
            .is_none()
        {
            return Err(DnsServiceError::NotFound);
        }
        let zone_ids: Vec<Uuid> = self
            .zones
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .iter()
            .filter_map(|(zone_id, (account_id, _))| (*account_id == id).then_some(*zone_id))
            .collect();
        self.zones
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .retain(|_, (account_id, _)| *account_id != id);
        self.records
            .lock()
            .map_err(|_| DnsServiceError::Repository)?
            .retain(|zone_id, _| !zone_ids.contains(zone_id));
        Ok(())
    }
}

/// SQLite DNS repository.
pub struct SqliteDnsRepository {
    pool: Pool<Sqlite>,
}
impl SqliteDnsRepository {
    /// Construct from the application pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl DnsRepository for SqliteDnsRepository {
    async fn save_account(&self, stored: &StoredProviderAccount) -> Result<(), DnsServiceError> {
        let capabilities = serde_json::to_string(&stored.account.capabilities)
            .map_err(|_| DnsServiceError::Repository)?;
        sqlx::query("INSERT INTO dns_provider_accounts(id,kind,name,capabilities,enabled,encrypted_credential) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,name=excluded.name,capabilities=excluded.capabilities,enabled=excluded.enabled,encrypted_credential=excluded.encrypted_credential")
            .bind(stored.account.id.to_string()).bind(&stored.account.kind).bind(&stored.account.name).bind(capabilities).bind(stored.account.enabled).bind(&stored.encrypted_credential).execute(&self.pool).await.map_err(|_| DnsServiceError::Repository)?;
        Ok(())
    }

    async fn account(&self, id: Uuid) -> Result<StoredProviderAccount, DnsServiceError> {
        let row = sqlx::query("SELECT id,kind,name,capabilities,enabled,encrypted_credential FROM dns_provider_accounts WHERE id=?").bind(id.to_string()).fetch_optional(&self.pool).await.map_err(|_| DnsServiceError::Repository)?.ok_or(DnsServiceError::NotFound)?;
        stored_account(&row)
    }

    async fn save_zone(
        &self,
        account_id: Uuid,
        zone: &ProviderZone,
    ) -> Result<(), DnsServiceError> {
        sqlx::query("INSERT INTO dns_zones(id,account_id,remote_id,name,remote_version,last_success,drift_status) VALUES(?,?,?,?,?,CURRENT_TIMESTAMP,'synchronized') ON CONFLICT(id) DO UPDATE SET remote_id=excluded.remote_id,name=excluded.name,remote_version=excluded.remote_version,last_success=CURRENT_TIMESTAMP,drift_status='synchronized'")
            .bind(zone.id.to_string()).bind(account_id.to_string()).bind(&zone.remote_id).bind(zone.name.as_str()).bind(zone.remote_version.as_str()).execute(&self.pool).await.map_err(|_| DnsServiceError::Repository)?;
        Ok(())
    }

    async fn import_records(
        &self,
        zone_id: Uuid,
        records: &[RemoteRecord],
    ) -> Result<(), DnsServiceError> {
        for record in records {
            let data =
                serde_json::to_string(&record.data).map_err(|_| DnsServiceError::Repository)?;
            sqlx::query("INSERT INTO dns_records(zone_id,remote_id,name,data,ttl,remote_version) VALUES(?,?,?,?,?,?) ON CONFLICT(zone_id,remote_id) DO UPDATE SET name=excluded.name,data=excluded.data,ttl=excluded.ttl,remote_version=excluded.remote_version")
            .bind(zone_id.to_string()).bind(&record.remote_id).bind(record.name.as_str()).bind(data).bind(i64::from(record.ttl)).bind(record.remote_version.as_str()).execute(&self.pool).await.map_err(|_| DnsServiceError::Repository)?;
        }
        Ok(())
    }

    async fn local_records(&self, zone_id: Uuid) -> Result<Vec<RemoteRecord>, DnsServiceError> {
        let rows = sqlx::query("SELECT remote_id,name,data,ttl,remote_version FROM dns_records WHERE zone_id=? ORDER BY name,remote_id").bind(zone_id.to_string()).fetch_all(&self.pool).await.map_err(|_| DnsServiceError::Repository)?;
        rows.iter().map(remote_record).collect()
    }

    async fn accounts(&self) -> Result<Vec<ProviderAccount>, DnsServiceError> {
        let rows=sqlx::query("SELECT id,kind,name,capabilities,enabled,encrypted_credential FROM dns_provider_accounts ORDER BY name").fetch_all(&self.pool).await.map_err(|_| DnsServiceError::Repository)?;
        rows.iter()
            .map(|row| stored_account(row).map(|stored| stored.account))
            .collect()
    }

    async fn zones(&self) -> Result<Vec<ProviderZone>, DnsServiceError> {
        let rows=sqlx::query("SELECT id,remote_id,name,remote_version,last_success,last_error,drift_status FROM dns_zones ORDER BY name").fetch_all(&self.pool).await.map_err(|_| DnsServiceError::Repository)?;
        rows.iter().map(provider_zone).collect()
    }

    async fn zone_account(
        &self,
        zone_id: Uuid,
    ) -> Result<(ProviderZone, StoredProviderAccount), DnsServiceError> {
        let row=sqlx::query("SELECT id,account_id,remote_id,name,remote_version,last_success,last_error,drift_status FROM dns_zones WHERE id=?").bind(zone_id.to_string()).fetch_optional(&self.pool).await.map_err(|_| DnsServiceError::Repository)?.ok_or(DnsServiceError::NotFound)?;
        let account_id = Uuid::parse_str(
            row.try_get("account_id")
                .map_err(|_| DnsServiceError::Repository)?,
        )
        .map_err(|_| DnsServiceError::Repository)?;
        Ok((provider_zone(&row)?, self.account(account_id).await?))
    }

    async fn delete_local_record(
        &self,
        zone_id: Uuid,
        remote_id: &str,
    ) -> Result<(), DnsServiceError> {
        sqlx::query("DELETE FROM dns_records WHERE zone_id=? AND remote_id=?")
            .bind(zone_id.to_string())
            .bind(remote_id)
            .execute(&self.pool)
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        Ok(())
    }

    async fn delete_account(&self, id: Uuid) -> Result<(), DnsServiceError> {
        let result = sqlx::query("DELETE FROM dns_provider_accounts WHERE id=?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        if result.rows_affected() == 0 {
            return Err(DnsServiceError::NotFound);
        }
        Ok(())
    }
}
fn stored_account(row: &sqlx::sqlite::SqliteRow) -> Result<StoredProviderAccount, DnsServiceError> {
    let id: String = row.try_get("id").map_err(|_| DnsServiceError::Repository)?;
    let capabilities: String = row
        .try_get("capabilities")
        .map_err(|_| DnsServiceError::Repository)?;
    Ok(StoredProviderAccount {
        account: ProviderAccount {
            id: Uuid::parse_str(&id).map_err(|_| DnsServiceError::Repository)?,
            kind: row
                .try_get("kind")
                .map_err(|_| DnsServiceError::Repository)?,
            name: row
                .try_get("name")
                .map_err(|_| DnsServiceError::Repository)?,
            capabilities: serde_json::from_str(&capabilities)
                .map_err(|_| DnsServiceError::Repository)?,
            enabled: row
                .try_get("enabled")
                .map_err(|_| DnsServiceError::Repository)?,
        },
        encrypted_credential: row
            .try_get("encrypted_credential")
            .map_err(|_| DnsServiceError::Repository)?,
    })
}
fn provider_zone(row: &sqlx::sqlite::SqliteRow) -> Result<ProviderZone, DnsServiceError> {
    let id: String = row.try_get("id").map_err(|_| DnsServiceError::Repository)?;
    let mut zone = ProviderZone::new(
        Uuid::parse_str(&id).map_err(|_| DnsServiceError::Repository)?,
        row.try_get::<String, _>("remote_id")
            .map_err(|_| DnsServiceError::Repository)?,
        openpanel_domain::dns::DnsName::new(
            row.try_get::<String, _>("name")
                .map_err(|_| DnsServiceError::Repository)?,
        )
        .map_err(|_| DnsServiceError::Repository)?,
        openpanel_domain::dns::RemoteVersion::new(
            row.try_get::<String, _>("remote_version")
                .map_err(|_| DnsServiceError::Repository)?,
        )
        .map_err(|_| DnsServiceError::Repository)?,
    );
    zone.last_success = row
        .try_get("last_success")
        .map_err(|_| DnsServiceError::Repository)?;
    zone.last_error = row
        .try_get("last_error")
        .map_err(|_| DnsServiceError::Repository)?;
    zone.drift_status = row
        .try_get("drift_status")
        .map_err(|_| DnsServiceError::Repository)?;
    Ok(zone)
}
fn remote_record(row: &sqlx::sqlite::SqliteRow) -> Result<RemoteRecord, DnsServiceError> {
    let data: String = row
        .try_get("data")
        .map_err(|_| DnsServiceError::Repository)?;
    let ttl: i64 = row
        .try_get("ttl")
        .map_err(|_| DnsServiceError::Repository)?;
    Ok(RemoteRecord {
        remote_id: row
            .try_get("remote_id")
            .map_err(|_| DnsServiceError::Repository)?,
        name: openpanel_domain::dns::DnsName::new(
            row.try_get::<String, _>("name")
                .map_err(|_| DnsServiceError::Repository)?,
        )
        .map_err(|_| DnsServiceError::Repository)?,
        data: serde_json::from_str(&data).map_err(|_| DnsServiceError::Repository)?,
        ttl: u32::try_from(ttl).map_err(|_| DnsServiceError::Repository)?,
        remote_version: openpanel_domain::dns::RemoteVersion::new(
            row.try_get::<String, _>("remote_version")
                .map_err(|_| DnsServiceError::Repository)?,
        )
        .map_err(|_| DnsServiceError::Repository)?,
    })
}
