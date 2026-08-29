//! Status page service: enable/disable, slug rotation, per-entry
//! publish + label, and a projector that turns `CheckResult`
//! history into per-entry incidents and 90-day uptime bars.

use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    CheckResult, CheckStatus, Role, SyntheticCheck, SyntheticRepository, User,
    synthetic_monitoring::{
        DailyBar, Incident, Slug, StatusEntry, StatusPage, StatusPageError,
        StatusPageRepository, derive_incidents, uptime_bars_90d,
    },
};
use rand::RngCore;
use uuid::Uuid;

use crate::synthetic_monitoring::SqliteStatusPageRepository;

/// Bundle of read-model slices returned for the public page.
#[derive(Debug, Clone)]
pub struct PublicStatusView {
    /// Entries that are currently visible on the page.
    pub entries: Vec<EntryView>,
    /// Per-entry ongoing or recent incidents (across all entries).
    pub incidents: Vec<Incident>,
    /// Audit-friendly timestamp for the read.
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

/// One rendered entry: the operator-chosen label and the latest
/// observed status (with optional latency).
#[derive(Debug, Clone)]
pub struct EntryView {
    /// Synthetic check id (not exposed to anonymous visitors).
    pub check_id: Uuid,
    /// Operator-chosen label that replaces the target on the page.
    pub label: String,
    /// Most recent observed status for this entry.
    pub current_status: CheckStatus,
    /// When the entry's check last ran.
    pub last_ran_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 90-day daily uptime bars (most-recent day last).
    pub uptime_bars: Vec<DailyBar>,
}

impl EntryView {
    /// Worst (highest-severity) status across the entry's daily bars.
    pub fn worst(&self) -> CheckStatus {
        let rank = |s: CheckStatus| match s {
            CheckStatus::Ok => 0,
            CheckStatus::Warn => 1,
            CheckStatus::Fail => 2,
        };
        self.uptime_bars
            .iter()
            .filter_map(|b| b.uptime)
            .map(|up| {
                if up >= 0.999 {
                    CheckStatus::Ok
                } else if up >= 0.5 {
                    CheckStatus::Warn
                } else {
                    CheckStatus::Fail
                }
            })
            .fold(CheckStatus::Ok, |acc, s| {
                if rank(s) > rank(acc) {
                    s
                } else {
                    acc
                }
            })
    }
}

/// Status page composition root.
pub struct StatusPageService {
    repo: Arc<SqliteStatusPageRepository>,
    synth: Arc<dyn SyntheticRepository>,
    audit: Arc<dyn AuditService>,
}

impl StatusPageService {
    /// Construct a service over the page repo and the synthetic
    /// check repo (used by the projector for daily bars / incidents).
    pub fn new(
        repo: Arc<SqliteStatusPageRepository>,
        synth: Arc<dyn SyntheticRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo,
            synth,
            audit,
        }
    }

    /// Load the page aggregate.
    pub async fn get(&self) -> Result<StatusPage, StatusPageError> {
        self.repo.load().await
    }

    /// List all configured synthetic checks (admin UI dropdown).
    pub async fn list_available_checks(&self) -> Result<Vec<SyntheticCheck>, StatusPageError> {
        self.synth.list_checks().await.map_err(map_repo)
    }

    /// Enable the page (audited).
    pub async fn enable(&self, caller: &User) -> Result<StatusPage, StatusPageError> {
        require_admin(caller)?;
        let mut page = self.repo.load().await?;
        page.enabled = true;
        self.repo.save(&page).await?;
        self.audit_change(caller, "enabled", &page).await;
        Ok(page)
    }

    /// Disable the page (audited).
    pub async fn disable(&self, caller: &User) -> Result<StatusPage, StatusPageError> {
        require_admin(caller)?;
        let mut page = self.repo.load().await?;
        page.enabled = false;
        self.repo.save(&page).await?;
        self.audit_change(caller, "disabled", &page).await;
        Ok(page)
    }

    /// Rotate the slug, returning the new page. The previous URL
    /// becomes a 404 thereafter.
    pub async fn regenerate_slug(&self, caller: &User) -> Result<StatusPage, StatusPageError> {
        require_admin(caller)?;
        let mut page = self.repo.load().await?;
        page.slug = random_slug();
        self.repo.save(&page).await?;
        self.audit_change(caller, "slug_regenerated", &page).await;
        Ok(page)
    }

    /// Publish a check on the page; create the entry if absent or
    /// update the label if present.
    pub async fn publish(
        &self,
        caller: &User,
        check_id: Uuid,
        label: impl Into<String>,
    ) -> Result<StatusPage, StatusPageError> {
        require_admin(caller)?;
        let label = label.into();
        if label.trim().is_empty() {
            return Err(StatusPageError::Persistence("empty label".into()));
        }
        if self.synth.get_check(check_id).await?.is_none() {
            return Err(StatusPageError::CheckNotFound(check_id));
        }
        let mut page = self.repo.load().await?;
        if let Some(entry) = page.entries.iter_mut().find(|e| e.check_id == check_id) {
            entry.label = label;
        } else {
            page.entries.push(StatusEntry { check_id, label });
        }
        self.repo.save(&page).await?;
        self.audit_change(caller, "published", &page).await;
        Ok(page)
    }

    /// Remove a check from the page.
    pub async fn unpublish(
        &self,
        caller: &User,
        check_id: Uuid,
    ) -> Result<StatusPage, StatusPageError> {
        require_admin(caller)?;
        let mut page = self.repo.load().await?;
        let before = page.entries.len();
        page.entries.retain(|e| e.check_id != check_id);
        if page.entries.len() == before {
            return Ok(page);
        }
        self.repo.save(&page).await?;
        self.audit_change(caller, "unpublished", &page).await;
        Ok(page)
    }

    /// Build a read-model view for the public page. Returns
    /// `Err(StatusPageError::Disabled)` when the page is disabled
    /// so the caller can render an indistinguishable 404.
    pub async fn public_view(&self) -> Result<PublicStatusView, StatusPageError> {
        let page = self.repo.load().await?;
        if !page.enabled {
            return Err(StatusPageError::Disabled);
        }
        let today = Utc::now().date_naive();
        let mut entries = Vec::with_capacity(page.entries.len());
        let mut incidents = Vec::new();
        for entry in &page.entries {
            let limit: u32 = 5_000;
            let results = self
                .synth
                .list_results(entry.check_id, limit)
                .await
                .map_err(|e| StatusPageError::Persistence(e.0))?;
            let current_status = results
                .iter()
                .max_by_key(|r| r.ran_at)
                .map(|r| r.status)
                .unwrap_or(CheckStatus::Ok);
            let last_ran_at = results.iter().map(|r| r.ran_at).max();
            let bars = uptime_bars_90d(&results, entry.check_id, today);
            entries.push(EntryView {
                check_id: entry.check_id,
                label: entry.label.clone(),
                current_status,
                last_ran_at,
                uptime_bars: bars,
            });
            incidents.extend(derive_incidents(&results, entry.check_id));
        }
        Ok(PublicStatusView {
            entries,
            incidents,
            generated_at: Utc::now(),
        })
    }

    /// Like [`Self::public_view`] but enforces the page slug match
    /// for callers that already know the URL the visitor requested.
    pub async fn public_view_for(
        &self,
        slug: &str,
    ) -> Result<PublicStatusView, StatusPageError> {
        let page = self.repo.load().await?;
        if page.slug.as_str() != slug {
            return Err(StatusPageError::Disabled);
        }
        if !page.enabled {
            return Err(StatusPageError::Disabled);
        }
        self.public_view().await
    }

    /// Aggregate the recent `CheckResult`s across all checks the
    /// caller can see. Used by the public projector + admin read
    /// views.
    pub async fn all_recent_results(&self) -> Result<Vec<CheckResult>, StatusPageError> {
        let checks = self.synth.list_checks().await.map_err(map_repo)?;
        let mut out = Vec::new();
        for check in checks {
            let results = self
                .synth
                .list_results(check.id, 5_000)
                .await
                .map_err(map_repo)?;
            out.extend(results);
        }
        Ok(out)
    }

    async fn audit_change(&self, caller: &User, action: &str, page: &StatusPage) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::StatusPagePolicyChanged,
                    AuditOutcome::Success,
                )
                .target("status_page")
                .metadata(serde_json::json!({
                    "action": action,
                    "slug": page.slug.as_str(),
                    "enabled": page.enabled,
                    "entries": page.entries.len(),
                })),
            )
            .await;
    }
}

fn require_admin(caller: &User) -> Result<(), StatusPageError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(StatusPageError::Persistence("forbidden".into())),
    }
}

fn map_repo(error: openpanel_domain::RepoError) -> StatusPageError {
    StatusPageError::Persistence(error.0)
}

/// Generate an enumeration-resistant base32-nopadding slug from
/// 128 bits of OS entropy (26 base32 chars).
pub fn random_slug() -> Slug {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let mut out = String::with_capacity(26);
    let mut buffer: u64 = 0;
    let mut bits: u32 = 0;
    for byte in bytes.iter() {
        buffer = (buffer << 8) | u64::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    // Safe: our generator only emits characters in the alphabet.
    Slug::new(out).expect("generated slug is alphanumeric")
}

#[doc(hidden)]
pub fn _today() -> NaiveDate {
    Utc::now().date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_slug_is_alphanumeric_and_26_chars() {
        let slug = random_slug();
        let s = slug.as_str();
        assert_eq!(s.len(), 26);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn random_slugs_are_unique_over_many_draws() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(random_slug().as_str().to_string()));
        }
    }

    #[test]
    fn entry_view_worst_promotes_on_partial_uptime() {
        let today = _today();
        let bars: Vec<DailyBar> = (0..90)
            .map(|offset| DailyBar {
                day: today - chrono::Duration::days(89 - offset),
                uptime: Some(if offset == 0 { 0.5 } else { 1.0 }),
            })
            .collect();
        let view = EntryView {
            check_id: Uuid::new_v4(),
            label: "API".into(),
            current_status: CheckStatus::Ok,
            last_ran_at: None,
            uptime_bars: bars,
        };
        assert_eq!(view.worst(), CheckStatus::Warn);
    }
}