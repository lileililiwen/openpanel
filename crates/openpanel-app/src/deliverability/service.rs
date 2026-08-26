//! Email deliverability application layer: blocklist checks,
//! DMARC report ingestion with retention, and composition.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::deliverability::{BlocklistZone, Listing, ResolverPort, parse_dmarc_report};
use sqlx::SqlitePool;

/// Blocklist checks and DMARC report ingestion.
pub struct DeliverabilityService {
    pool: SqlitePool,
    audit: Arc<dyn AuditService>,
    resolver: Arc<dyn ResolverPort>,
    zones: Vec<BlocklistZone>,
}

impl DeliverabilityService {
    /// Construct with the resolver port and configured zones.
    pub fn new(
        pool: SqlitePool,
        audit: Arc<dyn AuditService>,
        resolver: Arc<dyn ResolverPort>,
        zones: Vec<BlocklistZone>,
    ) -> Self {
        Self {
            pool,
            audit,
            resolver,
            zones,
        }
    }

    /// Check one address against every configured zone, upserting
    /// listing state. Returns true when any zone lists the address.
    pub async fn check_ip(
        &self,
        caller: &openpanel_domain::User,
        ip: std::net::IpAddr,
    ) -> Result<bool, String> {
        let now = Utc::now();
        let mut listed_any = false;
        for zone in &self.zones {
            let listed =
                openpanel_domain::deliverability::is_listed(self.resolver.as_ref(), ip, zone)
                    .await?;
            let mut listing = self
                .load_listing(ip, &zone.0)
                .await?
                .unwrap_or_else(|| Listing::listed(ip, &zone.0, now));
            listing.upsert(listed, now);
            if listed {
                listed_any = true;
            }
            sqlx::query(
                "INSERT INTO deliverability_listings \
                 (ip, zone, first_seen, last_seen, resolved_at) VALUES (?, ?, ?, ?, ?) \
                 ON CONFLICT(ip, zone) DO UPDATE SET last_seen = excluded.last_seen, \
                 resolved_at = excluded.resolved_at",
            )
            .bind(ip.to_string())
            .bind(&zone.0)
            .bind(listing.first_seen().to_rfc3339())
            .bind(listing.last_seen().to_rfc3339())
            .bind(listing.resolved_at().map(|t| t.to_rfc3339()))
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        }
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DeliverabilityChecked,
                    if listed_any {
                        AuditOutcome::Failure
                    } else {
                        AuditOutcome::Success
                    },
                )
                .target(ip.to_string()),
            )
            .await;
        Ok(listed_any)
    }

    async fn load_listing(
        &self,
        ip: std::net::IpAddr,
        zone: &str,
    ) -> Result<Option<Listing>, String> {
        let row = sqlx::query_as::<_, (String, String, String, Option<String>)>(
            "SELECT ip, zone, first_seen, resolved_at FROM deliverability_listings \
             WHERE ip = ? AND zone = ?",
        )
        .bind(ip.to_string())
        .bind(zone)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        row
            .map(|(ip, zone, first_seen, resolved_at)| {
                let first_seen = chrono::DateTime::parse_from_rfc3339(&first_seen)
                    .map(|t| t.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                let resolved_at = resolved_at.and_then(|t| {
                    chrono::DateTime::parse_from_rfc3339(&t)
                        .map(|t| t.with_timezone(&Utc))
                        .ok()
                });
                let parsed_ip: Result<std::net::IpAddr, String> =
                    ip.parse().map_err(|e: std::net::AddrParseError| e.to_string());
                parsed_ip.map(|parsed_ip| {
                    Listing::restore(
                        parsed_ip,
                        zone,
                        first_seen,
                        first_seen,
                        resolved_at,
                    )
                })
            })
            .transpose()
    }

    /// Parse and ingest a DMARC aggregate report, then prune stats
    /// older than 90 days. Returns the number of rows stored.
    pub async fn ingest_report(&self, xml: &str) -> Result<usize, String> {
        let stats = parse_dmarc_report(xml).map_err(|e| e.to_string())?;
        let day = Utc::now().date_naive().to_string();
        for stat in &stats {
            sqlx::query(
                "INSERT INTO dmarc_source_stats \
                 (day, source_ip, messages, dkim_pass, spf_pass) VALUES (?, ?, ?, ?, ?) \
                 ON CONFLICT(day, source_ip) DO UPDATE SET \
                 messages = messages + excluded.messages, \
                 dkim_pass = dkim_pass + excluded.dkim_pass, \
                 spf_pass = spf_pass + excluded.spf_pass",
            )
            .bind(&day)
            .bind(stat.source_ip().to_string())
            .bind(stat.messages() as i64)
            .bind(stat.dkim_pass() as i64)
            .bind(stat.spf_pass() as i64)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        }
        // 90-day retention prune.
        let cutoff = (Utc::now() - Duration::days(90)).date_naive().to_string();
        sqlx::query("DELETE FROM dmarc_source_stats WHERE day < ?")
            .bind(&cutoff)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(stats.len())
    }

    /// Aggregate message counts per source within `days`.
    pub async fn sources(&self, days: i64) -> Result<Vec<(String, i64)>, String> {
        let cutoff = (Utc::now() - Duration::days(days)).date_naive().to_string();
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT source_ip, SUM(messages) FROM dmarc_source_stats \
             WHERE day >= ? GROUP BY source_ip ORDER BY SUM(messages) DESC",
        )
        .bind(&cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows)
    }
}
