//! SQLite adapter for the wildcard SSL bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    AcmeEndpointMode, CertRequest, ChallengeKind, DnsLease, RepoError, WildcardRepository,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `WildcardRepository`.
#[derive(Clone)]
pub struct SqliteWildcardRepository {
    pool: SqlitePool,
}

impl SqliteWildcardRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl WildcardRepository for SqliteWildcardRepository {
    async fn save_request(&self, request: &CertRequest) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO wildcard_cert_requests \
             (id, site_id, apex, wildcard, challenge, endpoint_mode, dns_provider, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(request.id.to_string())
        .bind(request.site_id.to_string())
        .bind(&request.apex)
        .bind(if request.wildcard { 1 } else { 0 })
        .bind(request.challenge.as_str())
        .bind(request.endpoint_mode.as_str())
        .bind(&request.dns_provider)
        .bind(request.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_request(&self, id: Uuid) -> Result<Option<CertRequest>, RepoError> {
        let row = sqlx::query(
            "SELECT id, site_id, apex, wildcard, challenge, endpoint_mode, dns_provider, created_at \
             FROM wildcard_cert_requests WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_request).transpose()
    }

    async fn save_lease(&self, lease: &DnsLease) -> Result<(), RepoError> {
        let revoked_at = lease.revoked_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO dns_leases \
             (id, cert_request_id, fqdn, value, created_at, revoked_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(lease.id.to_string())
        .bind(lease.cert_request_id.to_string())
        .bind(&lease.fqdn)
        .bind(&lease.value)
        .bind(lease.created_at.to_rfc3339())
        .bind(revoked_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn revoke_lease(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("UPDATE dns_leases SET revoked_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find_lease_by_fqdn(&self, fqdn: &str) -> Result<Option<DnsLease>, RepoError> {
        let row = sqlx::query(
            "SELECT id, cert_request_id, fqdn, value, created_at, revoked_at \
             FROM dns_leases WHERE fqdn = ?",
        )
        .bind(fqdn)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_lease).transpose()
    }
}

fn decode_request(row: sqlx::sqlite::SqliteRow) -> Result<CertRequest, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let apex: String = row.try_get("apex").map_err(map_sqlx)?;
    let wildcard: i64 = row.try_get("wildcard").map_err(map_sqlx)?;
    let challenge: String = row.try_get("challenge").map_err(map_sqlx)?;
    let endpoint_mode: String = row.try_get("endpoint_mode").map_err(map_sqlx)?;
    let dns_provider: String = row.try_get("dns_provider").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let site_id = Uuid::parse_str(&site_id).map_err(|e| RepoError::new(e.to_string()))?;
    let challenge = match challenge.as_str() {
        "http_01" => ChallengeKind::Http01,
        "dns_01" => ChallengeKind::Dns01,
        other => return Err(RepoError::new(format!("unknown challenge: {other}"))),
    };
    let endpoint_mode = match endpoint_mode.as_str() {
        "staging" => AcmeEndpointMode::Staging,
        "production" => AcmeEndpointMode::Production,
        other => return Err(RepoError::new(format!("unknown endpoint_mode: {other}"))),
    };
    let created_at = parse_ts(&created_at)?;
    Ok(CertRequest {
        id,
        site_id,
        apex,
        wildcard: wildcard != 0,
        challenge,
        endpoint_mode,
        dns_provider,
        created_at,
    })
}

fn decode_lease(row: sqlx::sqlite::SqliteRow) -> Result<DnsLease, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let cert_request_id: String = row.try_get("cert_request_id").map_err(map_sqlx)?;
    let fqdn: String = row.try_get("fqdn").map_err(map_sqlx)?;
    let value: String = row.try_get("value").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let revoked_at: Option<String> = row.try_get("revoked_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let cert_request_id = Uuid::parse_str(&cert_request_id)
        .map_err(|e| RepoError::new(e.to_string()))?;
    let created_at = parse_ts(&created_at)?;
    let revoked_at = revoked_at.as_deref().map(parse_ts).transpose()?;
    Ok(DnsLease {
        id,
        cert_request_id,
        fqdn,
        value,
        created_at,
        revoked_at,
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
