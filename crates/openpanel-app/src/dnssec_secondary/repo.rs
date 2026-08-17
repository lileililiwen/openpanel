//! SQLite adapter for the DNSSEC + secondary DNS bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    DnsSecPolicy, DnsSecRepository, DsRecord, GlueRecord, KeyRole, RepoError, SecondaryNs,
    SigningAlgorithm, ZoneSigningKey,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `DnsSecRepository`.
#[derive(Clone)]
pub struct SqliteDnsSecRepository {
    pool: SqlitePool,
}

impl SqliteDnsSecRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl DnsSecRepository for SqliteDnsSecRepository {
    async fn save_policy(&self, policy: &DnsSecPolicy) -> Result<(), RepoError> {
        let enabled_at = policy.enabled_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO dnssec_policies (zone_id, enabled, algorithm, enabled_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(policy.zone_id.to_string())
        .bind(if policy.enabled { 1 } else { 0 })
        .bind(policy.algorithm.as_str())
        .bind(enabled_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_policy(&self, zone_id: Uuid) -> Result<Option<DnsSecPolicy>, RepoError> {
        let row = sqlx::query(
            "SELECT zone_id, enabled, algorithm, enabled_at FROM dnssec_policies WHERE zone_id = ?",
        )
        .bind(zone_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_policy).transpose()
    }

    async fn save_key(&self, key: &ZoneSigningKey) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO zone_signing_keys \
             (id, zone_id, role, algorithm, key_tag, public_digest, active, rollover_in_progress, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(key.id.to_string())
        .bind(key.zone_id.to_string())
        .bind(key.role.as_str())
        .bind(key.algorithm.as_str())
        .bind(key.key_tag as i64)
        .bind(&key.public_digest)
        .bind(if key.active { 1 } else { 0 })
        .bind(if key.rollover_in_progress { 1 } else { 0 })
        .bind(key.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_keys(&self, zone_id: Uuid) -> Result<Vec<ZoneSigningKey>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, zone_id, role, algorithm, key_tag, public_digest, active, rollover_in_progress, created_at \
             FROM zone_signing_keys WHERE zone_id = ?",
        )
        .bind(zone_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_key).collect()
    }

    async fn save_secondary(&self, secondary: &SecondaryNs) -> Result<(), RepoError> {
        let cidrs = serde_json::to_string(&secondary.allowed_cidrs)
            .map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO secondary_ns (id, zone_id, address, allowed_cidrs_json, added_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(secondary.id.to_string())
        .bind(secondary.zone_id.to_string())
        .bind(&secondary.address)
        .bind(cidrs)
        .bind(secondary.added_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_secondaries(&self, zone_id: Uuid) -> Result<Vec<SecondaryNs>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, zone_id, address, allowed_cidrs_json, added_at FROM secondary_ns WHERE zone_id = ?",
        )
        .bind(zone_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_secondary).collect()
    }

    async fn save_glue(&self, glue: &GlueRecord) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO glue_records (id, zone_id, name, a, aaaa) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(glue.id.to_string())
        .bind(glue.zone_id.to_string())
        .bind(&glue.name)
        .bind(&glue.a)
        .bind(&glue.aaaa)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_glue(&self, zone_id: Uuid) -> Result<Vec<GlueRecord>, RepoError> {
        let rows =
            sqlx::query("SELECT id, zone_id, name, a, aaaa FROM glue_records WHERE zone_id = ?")
                .bind(zone_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_glue).collect()
    }

    async fn save_ds(&self, ds: &DsRecord) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO ds_records (zone_id, key_tag, algorithm, digest_type, digest) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(ds.zone_id.to_string())
        .bind(ds.key_tag as i64)
        .bind(ds.algorithm as i64)
        .bind(ds.digest_type as i64)
        .bind(&ds.digest)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_ds(&self, zone_id: Uuid) -> Result<Vec<DsRecord>, RepoError> {
        let rows = sqlx::query(
            "SELECT zone_id, key_tag, algorithm, digest_type, digest FROM ds_records WHERE zone_id = ?",
        )
        .bind(zone_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_ds).collect()
    }
}

fn decode_policy(row: sqlx::sqlite::SqliteRow) -> Result<DnsSecPolicy, RepoError> {
    let zone_id: String = row.try_get("zone_id").map_err(map_sqlx)?;
    let enabled: i64 = row.try_get("enabled").map_err(map_sqlx)?;
    let algorithm: String = row.try_get("algorithm").map_err(map_sqlx)?;
    let enabled_at: Option<String> = row.try_get("enabled_at").map_err(map_sqlx)?;
    let zone_id = Uuid::parse_str(&zone_id).map_err(|e| RepoError::new(e.to_string()))?;
    let algorithm = match algorithm.as_str() {
        "rsasha256" => SigningAlgorithm::Rsasha256,
        "ecdsap256sha256" => SigningAlgorithm::Ecdsap256sha256,
        "ed25519" => SigningAlgorithm::Ed25519,
        other => return Err(RepoError::new(format!("unknown algorithm: {other}"))),
    };
    let enabled_at = enabled_at.as_deref().map(parse_ts).transpose()?;
    Ok(DnsSecPolicy {
        zone_id,
        enabled: enabled != 0,
        algorithm,
        enabled_at,
    })
}

fn decode_key(row: sqlx::sqlite::SqliteRow) -> Result<ZoneSigningKey, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let zone_id: String = row.try_get("zone_id").map_err(map_sqlx)?;
    let role: String = row.try_get("role").map_err(map_sqlx)?;
    let algorithm: String = row.try_get("algorithm").map_err(map_sqlx)?;
    let key_tag: i64 = row.try_get("key_tag").map_err(map_sqlx)?;
    let public_digest: String = row.try_get("public_digest").map_err(map_sqlx)?;
    let active: i64 = row.try_get("active").map_err(map_sqlx)?;
    let rollover_in_progress: i64 = row.try_get("rollover_in_progress").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let zone_id = Uuid::parse_str(&zone_id).map_err(|e| RepoError::new(e.to_string()))?;
    let role = match role.as_str() {
        "ksk" => KeyRole::Ksk,
        "zsk" => KeyRole::Zsk,
        other => return Err(RepoError::new(format!("unknown role: {other}"))),
    };
    let algorithm = match algorithm.as_str() {
        "rsasha256" => SigningAlgorithm::Rsasha256,
        "ecdsap256sha256" => SigningAlgorithm::Ecdsap256sha256,
        "ed25519" => SigningAlgorithm::Ed25519,
        other => return Err(RepoError::new(format!("unknown algorithm: {other}"))),
    };
    let created_at = parse_ts(&created_at)?;
    Ok(ZoneSigningKey {
        id,
        zone_id,
        role,
        algorithm,
        key_tag: key_tag as u32,
        public_digest,
        active: active != 0,
        rollover_in_progress: rollover_in_progress != 0,
        created_at,
    })
}

fn decode_secondary(row: sqlx::sqlite::SqliteRow) -> Result<SecondaryNs, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let zone_id: String = row.try_get("zone_id").map_err(map_sqlx)?;
    let address: String = row.try_get("address").map_err(map_sqlx)?;
    let cidrs_json: String = row.try_get("allowed_cidrs_json").map_err(map_sqlx)?;
    let added_at: String = row.try_get("added_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let zone_id = Uuid::parse_str(&zone_id).map_err(|e| RepoError::new(e.to_string()))?;
    let allowed_cidrs: Vec<String> = serde_json::from_str(&cidrs_json)
        .map_err(|e| RepoError::new(format!("invalid cidrs_json: {e}")))?;
    let added_at = parse_ts(&added_at)?;
    Ok(SecondaryNs {
        id,
        zone_id,
        address,
        allowed_cidrs,
        added_at,
    })
}

fn decode_glue(row: sqlx::sqlite::SqliteRow) -> Result<GlueRecord, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let zone_id: String = row.try_get("zone_id").map_err(map_sqlx)?;
    let name: String = row.try_get("name").map_err(map_sqlx)?;
    let a: Option<String> = row.try_get("a").map_err(map_sqlx)?;
    let aaaa: Option<String> = row.try_get("aaaa").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let zone_id = Uuid::parse_str(&zone_id).map_err(|e| RepoError::new(e.to_string()))?;
    Ok(GlueRecord {
        id,
        zone_id,
        name,
        a,
        aaaa,
    })
}

fn decode_ds(row: sqlx::sqlite::SqliteRow) -> Result<DsRecord, RepoError> {
    let zone_id: String = row.try_get("zone_id").map_err(map_sqlx)?;
    let key_tag: i64 = row.try_get("key_tag").map_err(map_sqlx)?;
    let algorithm: i64 = row.try_get("algorithm").map_err(map_sqlx)?;
    let digest_type: i64 = row.try_get("digest_type").map_err(map_sqlx)?;
    let digest: String = row.try_get("digest").map_err(map_sqlx)?;
    let zone_id = Uuid::parse_str(&zone_id).map_err(|e| RepoError::new(e.to_string()))?;
    Ok(DsRecord {
        zone_id,
        key_tag: key_tag as u32,
        algorithm: algorithm as u8,
        digest_type: digest_type as u8,
        digest,
    })
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("invalid timestamp: {e}")))
}

fn map_sqlx(e: sqlx::Error) -> RepoError {
    RepoError::new(e.to_string())
}
