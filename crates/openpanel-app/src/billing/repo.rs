//! SQLite adapter for the reseller billing bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    BillingRepository, BillingStatus, Chargeback, ChargebackLine, Integration, RepoError,
    UsageMeter, UsageUnit,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `BillingRepository`.
#[derive(Clone)]
pub struct SqliteBillingRepository {
    pool: SqlitePool,
}

impl SqliteBillingRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl BillingRepository for SqliteBillingRepository {
    async fn save_meter(&self, meter: &UsageMeter) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO usage_meters \
             (id, owner_id, unit, quantity, period_start, period_end, closed, recorded_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(meter.id.to_string())
        .bind(meter.owner_id.to_string())
        .bind(meter.unit.as_str())
        .bind(meter.quantity as i64)
        .bind(meter.period_start.to_rfc3339())
        .bind(meter.period_end.to_rfc3339())
        .bind(if meter.closed { 1 } else { 0 })
        .bind(meter.recorded_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_meters(&self, owner_id: Uuid) -> Result<Vec<UsageMeter>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, owner_id, unit, quantity, period_start, period_end, closed, recorded_at \
             FROM usage_meters WHERE owner_id = ? ORDER BY period_start",
        )
        .bind(owner_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_meter).collect()
    }

    async fn save_chargeback(&self, chargeback: &Chargeback) -> Result<(), RepoError> {
        let lines_json =
            serde_json::to_string(&chargeback.lines).map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO chargebacks \
             (id, owner_id, period_start, period_end, amount_minor, currency, lines_json, finalised) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(chargeback.id.to_string())
        .bind(chargeback.owner_id.to_string())
        .bind(chargeback.period_start.to_rfc3339())
        .bind(chargeback.period_end.to_rfc3339())
        .bind(chargeback.amount_minor as i64)
        .bind(&chargeback.currency)
        .bind(lines_json)
        .bind(if chargeback.finalised { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_chargebacks(&self, owner_id: Uuid) -> Result<Vec<Chargeback>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, owner_id, period_start, period_end, amount_minor, currency, lines_json, finalised \
             FROM chargebacks WHERE owner_id = ? ORDER BY period_start",
        )
        .bind(owner_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_chargeback).collect()
    }

    async fn save_integration(&self, integration: &Integration) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO billing_integrations \
             (id, name, webhook_url, webhook_secret, status, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(integration.id.to_string())
        .bind(&integration.name)
        .bind(&integration.webhook_url)
        .bind(&integration.webhook_secret)
        .bind(integration.status.as_str())
        .bind(integration.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_integrations(&self) -> Result<Vec<Integration>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, name, webhook_url, webhook_secret, status, created_at \
             FROM billing_integrations ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_integration).collect()
    }

    async fn get_integration(&self, id: Uuid) -> Result<Option<Integration>, RepoError> {
        let row = sqlx::query(
            "SELECT id, name, webhook_url, webhook_secret, status, created_at \
             FROM billing_integrations WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_integration).transpose()
    }
}

fn decode_meter(row: sqlx::sqlite::SqliteRow) -> Result<UsageMeter, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let owner_id: String = row.try_get("owner_id").map_err(map_sqlx)?;
    let unit: String = row.try_get("unit").map_err(map_sqlx)?;
    let quantity: i64 = row.try_get("quantity").map_err(map_sqlx)?;
    let period_start: String = row.try_get("period_start").map_err(map_sqlx)?;
    let period_end: String = row.try_get("period_end").map_err(map_sqlx)?;
    let closed: i64 = row.try_get("closed").map_err(map_sqlx)?;
    let recorded_at: String = row.try_get("recorded_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let owner_id = Uuid::parse_str(&owner_id).map_err(|e| RepoError::new(e.to_string()))?;
    let unit = match unit.as_str() {
        "gb" => UsageUnit::Gigabytes,
        "cpu_minutes" => UsageUnit::CpuMinutes,
        "requests" => UsageUnit::Requests,
        "sites" => UsageUnit::Sites,
        other => return Err(RepoError::new(format!("unknown unit: {other}"))),
    };
    let period_start = parse_ts(&period_start)?;
    let period_end = parse_ts(&period_end)?;
    let recorded_at = parse_ts(&recorded_at)?;
    Ok(UsageMeter {
        id,
        owner_id,
        unit,
        quantity: quantity as u64,
        period_start,
        period_end,
        closed: closed != 0,
        recorded_at,
    })
}

fn decode_chargeback(row: sqlx::sqlite::SqliteRow) -> Result<Chargeback, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let owner_id: String = row.try_get("owner_id").map_err(map_sqlx)?;
    let period_start: String = row.try_get("period_start").map_err(map_sqlx)?;
    let period_end: String = row.try_get("period_end").map_err(map_sqlx)?;
    let amount_minor: i64 = row.try_get("amount_minor").map_err(map_sqlx)?;
    let currency: String = row.try_get("currency").map_err(map_sqlx)?;
    let lines_json: String = row.try_get("lines_json").map_err(map_sqlx)?;
    let finalised: i64 = row.try_get("finalised").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let owner_id = Uuid::parse_str(&owner_id).map_err(|e| RepoError::new(e.to_string()))?;
    let period_start = parse_ts(&period_start)?;
    let period_end = parse_ts(&period_end)?;
    let lines: Vec<ChargebackLine> = serde_json::from_str(&lines_json)
        .map_err(|e| RepoError::new(format!("invalid lines_json: {e}")))?;
    Ok(Chargeback {
        id,
        owner_id,
        period_start,
        period_end,
        amount_minor: amount_minor as u64,
        currency,
        lines,
        finalised: finalised != 0,
    })
}

fn decode_integration(row: sqlx::sqlite::SqliteRow) -> Result<Integration, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let name: String = row.try_get("name").map_err(map_sqlx)?;
    let webhook_url: String = row.try_get("webhook_url").map_err(map_sqlx)?;
    let webhook_secret: String = row.try_get("webhook_secret").map_err(map_sqlx)?;
    let status: String = row.try_get("status").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let status = match status.as_str() {
        "enabled" => BillingStatus::Enabled,
        "disabled" => BillingStatus::Disabled,
        other => return Err(RepoError::new(format!("unknown status: {other}"))),
    };
    let created_at = parse_ts(&created_at)?;
    Ok(Integration {
        id,
        name,
        webhook_url,
        webhook_secret,
        status,
        created_at,
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
