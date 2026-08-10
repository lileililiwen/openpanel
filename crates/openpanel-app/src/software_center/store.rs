//! Catalog store: normalized SQLite-backed aggregator for the Software Center.
//!
//! The store is the panel's view of the active catalog. It ingests
//! validated `CatalogManifest` values from any `CatalogSource`, persists
//! them in the V002 schema, answers search/filter queries, exposes
//! diagnostics, and supports the per-version pre-flight compatibility
//! check. The store is *data only* — it never executes content from the
//! manifest.
#![allow(missing_docs)]

use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use openpanel_domain::software_center::{
    CatalogEntryRecipe, CatalogHit, CatalogManifest, CatalogQuery, CatalogSearchPage, Category,
    EntryKind, Homepage, Provenance, RecipeError,
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use super::{SoftwareCenterError, source::CatalogSource};

/// Default catalog URL used when the operator has not configured one.
/// Points at the open-source catalog hosted on the project CDN; the
/// value is overridable through `OPENPANEL__SOFTWARE__CATALOG_URL`.
pub fn default_catalog_url() -> String {
    std::env::var("OPENPANEL__SOFTWARE__CATALOG_URL")
        .unwrap_or_else(|_| "https://catalog.openpanel.dev/v1/manifest.json".to_owned())
}

/// Activation record for the current snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogActivation {
    pub source_url: String,
    pub source_id: String,
    pub manifest_digest: String,
    pub entry_count: usize,
    pub activated_at: String,
    pub embedded: bool,
    pub age_seconds: i64,
}

/// Diagnostics for the catalog layer, suitable for the diagnostics strip
/// and the `software diagnostics` CLI command.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogDiagnostics {
    pub source_url: String,
    pub source_id: String,
    pub manifest_digest: String,
    pub entry_count: usize,
    pub activated_at: String,
    pub age_seconds: i64,
    pub last_refresh: Option<LastRefresh>,
    pub staleness_threshold_seconds: i64,
    pub stale: bool,
}

/// Last refresh attempt outcome (success or failure).
#[derive(Debug, Clone, Serialize)]
pub struct LastRefresh {
    pub attempted_at: String,
    pub outcome: String,
    pub manifest_digest: Option<String>,
    pub error: Option<String>,
}

/// Refresh outcome returned to callers.
#[derive(Debug, Clone, Serialize)]
pub struct RefreshOutcome {
    pub manifest_digest: String,
    pub entry_count: usize,
    pub source_url: String,
    pub activated_at: String,
}

/// Pre-flight compatibility report. A non-empty `errors` list means the
/// plan MUST NOT proceed.
#[derive(Debug, Clone, Serialize, Default)]
pub struct CompatibilityReport {
    pub errors: Vec<CompatibilityIssue>,
    pub warnings: Vec<CompatibilityIssue>,
}

impl CompatibilityReport {
    /// True when the report contains no errors.
    pub fn is_compatible(&self) -> bool {
        self.errors.is_empty()
    }
}

/// One compatibility issue.
#[derive(Debug, Clone, Serialize)]
pub struct CompatibilityIssue {
    pub code: String,
    pub message: String,
    pub related: Option<String>,
}

/// Wizard state. Held server-side in a signed cookie; the in-memory
/// representation is a strict subset of what the cookie carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardState {
    pub entry_id: String,
    pub version: String,
    pub site_action: WizardSiteAction,
    pub site_id: Option<uuid::Uuid>,
    pub new_site_domain: Option<String>,
    pub php_version: Option<String>,
    pub database_action: WizardDatabaseAction,
    pub existing_database_id: Option<uuid::Uuid>,
    pub locale: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WizardSiteAction {
    UseExisting,
    CreateNew,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WizardDatabaseAction {
    Autogenerate,
    UseExisting,
}

impl WizardState {
    /// Build a new wizard state with sensible defaults.
    pub fn new(entry_id: String, version: String, now: u64) -> Self {
        Self {
            entry_id,
            version,
            site_action: WizardSiteAction::CreateNew,
            site_id: None,
            new_site_domain: None,
            php_version: None,
            database_action: WizardDatabaseAction::Autogenerate,
            existing_database_id: None,
            locale: "en_US".to_owned(),
            updated_at: now,
        }
    }
}

/// Bounded staleness threshold (24 hours).
pub const DEFAULT_STALENESS_SECONDS: i64 = 24 * 60 * 60;

/// Storefront-facing view of a single entry, used by the API and the
/// web module.
#[derive(Debug, Clone, Serialize)]
pub struct StorefrontEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub long_description: String,
    pub category: Category,
    pub kind: EntryKind,
    pub license: String,
    pub developer: String,
    pub homepage: String,
    pub icon: Option<String>,
    pub latest_version: String,
    pub versions: Vec<StorefrontVersion>,
    pub tags: Vec<String>,
    pub dependencies: Vec<String>,
    pub conflicts: Vec<String>,
    pub install_state: String,
    pub installed_version: Option<String>,
    pub size_bytes: u64,
    pub provenance: Provenance,
}

/// Bounded version projection.
#[derive(Debug, Clone, Serialize)]
pub struct StorefrontVersion {
    pub version: String,
    pub size_bytes: u64,
    pub supports_php: Vec<String>,
    pub released_at: Option<String>,
    pub changelog_url: Option<String>,
    pub is_latest: bool,
}

/// Persisted catalog store. Constructed once per service instance.
pub struct SoftwareCatalogStore {
    pool: Option<SqlitePool>,
    staleness_threshold: i64,
}

impl SoftwareCatalogStore {
    /// Build a store bound to an optional SQLite pool. When the pool is
    /// `None`, the store is read-only and can serve the embedded seed.
    pub fn new(pool: Option<SqlitePool>) -> Self {
        let staleness = std::env::var("OPENPANEL__SOFTWARE__CATALOG_MAX_AGE")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(DEFAULT_STALENESS_SECONDS);
        Self {
            pool,
            staleness_threshold: staleness,
        }
    }

    /// True when the store has an active snapshot in SQLite.
    pub async fn has_active_snapshot(&self) -> bool {
        let Some(pool) = &self.pool else {
            return false;
        };
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM software_entries")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        count > 0
    }

    /// Atomically activate a fetched manifest. Returns the activation
    /// record on success. Existing entries are replaced in a single
    /// transaction; the previous active snapshot is removed.
    pub async fn activate(
        &self,
        source_id: &str,
        fetched: &super::source::FetchedManifest,
    ) -> Result<CatalogActivation, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let activated_at = chrono::Utc::now().to_rfc3339();
        let mut transaction = pool
            .begin()
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        sqlx::query("DELETE FROM software_entries")
            .execute(&mut *transaction)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        for entry in &fetched.manifest.entries {
            let latest = entry.latest_version();
            let search_text = build_search_text(entry);
            let size_bytes: i64 = latest.size_bytes.try_into().unwrap_or(0);
            sqlx::query(
                "INSERT INTO software_entries(id, name, description, long_description, category, kind, license, developer, homepage, icon, size_bytes, latest_version, search_text, activated_at, source_url, manifest_digest, embedded) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(&entry.id)
            .bind(&entry.name)
            .bind(&entry.description)
            .bind(&entry.long_description)
            .bind(entry.category.slug())
            .bind(entry.kind.as_str())
            .bind(entry.license.as_str())
            .bind(&entry.developer)
            .bind(entry.homepage.as_str())
            .bind(entry.icon.as_deref())
            .bind(size_bytes)
            .bind(&latest.version)
            .bind(&search_text)
            .bind(&activated_at)
            .bind(&fetched.source_url)
            .bind(fetched.manifest.digest())
            .bind(if source_id == "embedded" { 1_i64 } else { 0_i64 })
            .execute(&mut *transaction)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
            for (index, version) in entry.versions.iter().enumerate() {
                let artifact_json = version
                    .artifact
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|_| SoftwareCenterError::Repository)?;
                let packages_json = serde_json::to_string(&version.packages)
                    .map_err(|_| SoftwareCenterError::Repository)?;
                let supports_php_json = serde_json::to_string(&version.supports_php)
                    .map_err(|_| SoftwareCenterError::Repository)?;
                sqlx::query(
                    "INSERT INTO software_entry_versions(entry_id, version, size_bytes, changelog_url, artifact_json, packages_json, supports_php_json, released_at, is_latest) VALUES(?,?,?,?,?,?,?,?,?)",
                )
                .bind(&entry.id)
                .bind(&version.version)
                .bind(version.size_bytes as i64)
                .bind(version.changelog_url.as_ref().map(Homepage::as_str))
                .bind(artifact_json)
                .bind(&packages_json)
                .bind(&supports_php_json)
                .bind(version.released_at.as_deref())
                .bind(if index == 0 { 1_i64 } else { 0_i64 })
                .execute(&mut *transaction)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
            }
            for tag in &entry.tags {
                sqlx::query(
                    "INSERT INTO software_entry_tags(entry_id, tag) VALUES(?,?) ON CONFLICT DO NOTHING",
                )
                .bind(&entry.id)
                .bind(tag.as_str())
                .execute(&mut *transaction)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
            }
            for dep in &entry.dependencies {
                sqlx::query(
                    "INSERT INTO software_entry_deps(entry_id, dep_id) VALUES(?,?) ON CONFLICT DO NOTHING",
                )
                .bind(&entry.id)
                .bind(dep)
                .execute(&mut *transaction)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
            }
            for conflict in &entry.conflicts {
                sqlx::query(
                    "INSERT INTO software_entry_conflicts(entry_id, conflict_id) VALUES(?,?) ON CONFLICT DO NOTHING",
                )
                .bind(&entry.id)
                .bind(conflict)
                .execute(&mut *transaction)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
            }
        }
        transaction
            .commit()
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        Ok(CatalogActivation {
            source_url: fetched.source_url.clone(),
            source_id: source_id.to_owned(),
            manifest_digest: fetched.manifest.digest(),
            entry_count: fetched.manifest.entries.len(),
            activated_at,
            embedded: source_id == "embedded",
            age_seconds: 0,
        })
    }

    /// Materialize the embedded seed into the store when no snapshot is
    /// active. Returns `None` when a snapshot is already active.
    pub async fn materialize_seed_if_empty(
        &self,
        embedded: &super::source::EmbeddedCatalogSource,
    ) -> Result<Option<CatalogActivation>, SoftwareCenterError> {
        if self.has_active_snapshot().await {
            return Ok(None);
        }
        // The store's tables may not exist yet (migrations run on first
        // boot or test setup). Detect a missing-table condition and
        // surface it as a Repository error so the caller can retry.
        let Some(pool) = self.pool.as_ref() else {
            return Ok(None);
        };
        let probe: Result<i64, _> = sqlx::query_scalar("SELECT COUNT(*) FROM software_entries")
            .fetch_one(pool)
            .await;
        if probe.is_err() {
            return Err(SoftwareCenterError::Repository);
        }
        let fetched = embedded.fetch(now_unix()?).await?;
        let activation = self.activate("embedded", &fetched).await?;
        Ok(Some(activation))
    }

    /// Run a search/filter query against the active snapshot.
    pub async fn search(
        &self,
        query: &CatalogQuery,
    ) -> Result<CatalogSearchPage, SoftwareCenterError> {
        use sqlx::QueryBuilder;
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;

        // Build the count query (without pagination).
        let mut count_qb: QueryBuilder<sqlx::Sqlite> =
            QueryBuilder::new("SELECT COUNT(*) AS c FROM software_entries WHERE 1=1");
        Self::apply_filters(&mut count_qb, query);
        let count_row = count_qb
            .build()
            .fetch_one(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        let total: i64 = count_row
            .try_get("c")
            .map_err(|_| SoftwareCenterError::Repository)?;
        let total = total.max(0) as usize;

        let mut page_qb: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
            "SELECT id, name, description, category, kind, license, developer, latest_version, icon, size_bytes FROM software_entries WHERE 1=1",
        );
        Self::apply_filters(&mut page_qb, query);
        match query.sort {
            CatalogSort::Name => {
                page_qb.push(" ORDER BY LOWER(name) ASC");
            }
            CatalogSort::Recent => {
                page_qb.push(" ORDER BY activated_at DESC");
            }
            CatalogSort::Size => {
                page_qb.push(" ORDER BY size_bytes DESC");
            }
        }
        let page = query.page.min(100);
        let page_size = query.page_size.clamp(1, 60);
        let offset = (page * page_size) as i64;
        page_qb.push(" LIMIT ").push_bind(page_size as i64);
        page_qb.push(" OFFSET ").push_bind(offset);

        let rows = page_qb
            .build()
            .fetch_all(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;

        let mut hits = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row
                .try_get("id")
                .map_err(|_| SoftwareCenterError::Repository)?;
            let tags = self.tags_for(&id).await?;
            let install_state = self.install_state_for(&id).await?;
            hits.push(CatalogHit {
                id,
                name: row
                    .try_get("name")
                    .map_err(|_| SoftwareCenterError::Repository)?,
                description: row
                    .try_get("description")
                    .map_err(|_| SoftwareCenterError::Repository)?,
                category: parse_category(
                    row.try_get::<String, _>("category")
                        .map_err(|_| SoftwareCenterError::Repository)?
                        .as_str(),
                )?,
                kind: parse_kind(
                    row.try_get::<String, _>("kind")
                        .map_err(|_| SoftwareCenterError::Repository)?
                        .as_str(),
                )?,
                license: row
                    .try_get("license")
                    .map_err(|_| SoftwareCenterError::Repository)?,
                developer: row
                    .try_get("developer")
                    .map_err(|_| SoftwareCenterError::Repository)?,
                latest_version: row
                    .try_get("latest_version")
                    .map_err(|_| SoftwareCenterError::Repository)?,
                tags,
                install_state,
                icon: row
                    .try_get::<Option<String>, _>("icon")
                    .map_err(|_| SoftwareCenterError::Repository)?,
                size_bytes: row
                    .try_get::<i64, _>("size_bytes")
                    .map_err(|_| SoftwareCenterError::Repository)?
                    as u64,
            });
        }
        Ok(CatalogSearchPage {
            total,
            page,
            page_size,
            sort: sort_label(query.sort),
            hits,
        })
    }

    fn apply_filters(qb: &mut sqlx::QueryBuilder<'_, sqlx::Sqlite>, query: &CatalogQuery) {
        if let Some(text) = query.text.as_ref().filter(|value| !value.trim().is_empty()) {
            let needle = format!("%{}%", text.trim().to_ascii_lowercase());
            qb.push(" AND (LOWER(search_text) LIKE ")
                .push_bind(needle.clone())
                .push(" OR LOWER(name) LIKE ")
                .push_bind(needle)
                .push(")");
        }
        if !query.categories.is_empty() {
            qb.push(" AND category IN (");
            let mut separated = qb.separated(", ");
            for category in &query.categories {
                separated.push_bind(category.slug().to_owned());
            }
            qb.push(")");
        }
        if !query.tags.is_empty() {
            qb.push(" AND id IN (SELECT entry_id FROM software_entry_tags WHERE tag IN (");
            let mut separated = qb.separated(", ");
            for tag in &query.tags {
                separated.push_bind(tag.as_str().to_owned());
            }
            qb.push(") GROUP BY entry_id HAVING COUNT(DISTINCT tag) = ")
                .push_bind(query.tags.len() as i64);
            qb.push(")");
        }
        if let Some(kind) = query.kind {
            qb.push(" AND kind = ").push_bind(kind.as_str().to_owned());
        }
        if query.installed_only {
            qb.push(" AND id IN (SELECT id FROM software_components WHERE managed=1)");
        }
        if query.update_available_only {
            qb.push(
                " AND id IN (SELECT id FROM software_components WHERE managed=1 AND version IS NOT NULL AND version != latest_version)",
            );
        }
    }

    /// Fetch a single entry by ID. Returns `None` when the entry does not
    /// exist in the active snapshot.
    pub async fn get_entry(
        &self,
        id: &str,
    ) -> Result<Option<StorefrontEntry>, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let row = sqlx::query(
            "SELECT id, name, description, long_description, category, kind, license, developer, homepage, icon, size_bytes, latest_version, activated_at, source_url, manifest_digest, embedded FROM software_entries WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?;
        let Some(row) = row else { return Ok(None) };
        let activated_at: String = row
            .try_get("activated_at")
            .map_err(|_| SoftwareCenterError::Repository)?;
        let source_url: String = row
            .try_get("source_url")
            .map_err(|_| SoftwareCenterError::Repository)?;
        let manifest_digest: String = row
            .try_get("manifest_digest")
            .map_err(|_| SoftwareCenterError::Repository)?;
        let embedded: i64 = row
            .try_get("embedded")
            .map_err(|_| SoftwareCenterError::Repository)?;
        let versions = self.versions_for(id).await?;
        let tags = self.tags_for(id).await?;
        let dependencies = self.deps_for(id).await?;
        let conflicts = self.conflicts_for(id).await?;
        let install_state = self.install_state_for(id).await?;
        let installed_version = self.installed_version_for(id).await?;
        Ok(Some(StorefrontEntry {
            id: id.to_owned(),
            name: row
                .try_get("name")
                .map_err(|_| SoftwareCenterError::Repository)?,
            description: row
                .try_get("description")
                .map_err(|_| SoftwareCenterError::Repository)?,
            long_description: row
                .try_get("long_description")
                .map_err(|_| SoftwareCenterError::Repository)?,
            category: parse_category(
                row.try_get::<String, _>("category")
                    .map_err(|_| SoftwareCenterError::Repository)?
                    .as_str(),
            )?,
            kind: parse_kind(
                row.try_get::<String, _>("kind")
                    .map_err(|_| SoftwareCenterError::Repository)?
                    .as_str(),
            )?,
            license: row
                .try_get("license")
                .map_err(|_| SoftwareCenterError::Repository)?,
            developer: row
                .try_get("developer")
                .map_err(|_| SoftwareCenterError::Repository)?,
            homepage: row
                .try_get("homepage")
                .map_err(|_| SoftwareCenterError::Repository)?,
            icon: row
                .try_get::<Option<String>, _>("icon")
                .map_err(|_| SoftwareCenterError::Repository)?,
            latest_version: row
                .try_get("latest_version")
                .map_err(|_| SoftwareCenterError::Repository)?,
            versions,
            tags,
            dependencies,
            conflicts,
            install_state,
            installed_version,
            size_bytes: row
                .try_get::<i64, _>("size_bytes")
                .map_err(|_| SoftwareCenterError::Repository)? as u64,
            provenance: Provenance {
                source_url,
                manifest_digest,
                activated_at,
                entry_count: self.count_entries().await?,
                embedded: embedded != 0,
            },
        }))
    }

    /// Compute the pre-flight compatibility report for a given entry,
    /// version, and host context. The report returns the typed errors
    /// and warnings that the storefront and the wizard surface.
    pub async fn compatibility_for(
        &self,
        entry_id: &str,
        version: &str,
        host: &CompatibilityHost,
        managed_php_versions: &[String],
        installed_components: &[(String, bool)],
    ) -> Result<CompatibilityReport, SoftwareCenterError> {
        let Some(entry) = self.get_entry(entry_id).await? else {
            return Ok(CompatibilityReport {
                errors: vec![CompatibilityIssue {
                    code: "entry_not_found".into(),
                    message: format!("entry {entry_id} not found"),
                    related: None,
                }],
                warnings: Vec::new(),
            });
        };
        let mut report = CompatibilityReport::default();
        let chosen = entry
            .versions
            .iter()
            .find(|v| v.version == version)
            .cloned();
        let Some(version) = chosen else {
            report.errors.push(CompatibilityIssue {
                code: "version_not_found".into(),
                message: format!("version {version} not found for {entry_id}"),
                related: Some(entry_id.to_owned()),
            });
            return Ok(report);
        };
        if !version.supports_php.is_empty()
            && let Some(php) = host.php_version.as_deref()
            && !version.supports_php.iter().any(|v| v == php)
        {
            report.errors.push(CompatibilityIssue {
                code: "unsupported_php".into(),
                message: format!(
                    "selected PHP {php} is not in the supported set {:?}",
                    version.supports_php
                ),
                related: Some("php_version".into()),
            });
        }
        if !version.supports_php.is_empty() && host.php_version.is_none() {
            for required in &version.supports_php {
                if !managed_php_versions.iter().any(|v| v == required) {
                    report.errors.push(CompatibilityIssue {
                        code: "missing_php_runtime".into(),
                        message: format!(
                            "PHP runtime {required} is required but not installed and not managed"
                        ),
                        related: Some(format!("php-{required}")),
                    });
                }
            }
        }
        for dep in &entry.dependencies {
            let installed = installed_components
                .iter()
                .any(|(id, managed)| id == dep && *managed);
            if !installed {
                report.errors.push(CompatibilityIssue {
                    code: "missing_dependency".into(),
                    message: format!("required dependency {dep} is not installed"),
                    related: Some(dep.clone()),
                });
            }
        }
        for conflict in &entry.conflicts {
            let installed = installed_components
                .iter()
                .any(|(id, managed)| id == conflict && *managed);
            if installed {
                report.errors.push(CompatibilityIssue {
                    code: "conflict_installed".into(),
                    message: format!("conflicting entry {conflict} is already managed"),
                    related: Some(conflict.clone()),
                });
            }
        }
        if entry.install_state == "unsupported" {
            report.warnings.push(CompatibilityIssue {
                code: "host_unsupported".into(),
                message: "host platform is not in the recipe's supported set".into(),
                related: None,
            });
        }
        Ok(report)
    }

    /// Build diagnostics for the diagnostics strip / CLI.
    pub async fn diagnostics(&self) -> Result<CatalogDiagnostics, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let row = sqlx::query(
            "SELECT activated_at, source_url, manifest_digest, embedded FROM software_entries LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?;
        let Some(row) = row else {
            return Ok(CatalogDiagnostics {
                source_url: String::new(),
                source_id: String::new(),
                manifest_digest: String::new(),
                entry_count: 0,
                activated_at: String::new(),
                age_seconds: 0,
                last_refresh: None,
                staleness_threshold_seconds: self.staleness_threshold,
                stale: true,
            });
        };
        let activated_at: String = row
            .try_get("activated_at")
            .map_err(|_| SoftwareCenterError::Repository)?;
        let source_url: String = row
            .try_get("source_url")
            .map_err(|_| SoftwareCenterError::Repository)?;
        let manifest_digest: String = row
            .try_get("manifest_digest")
            .map_err(|_| SoftwareCenterError::Repository)?;
        let embedded: i64 = row
            .try_get("embedded")
            .map_err(|_| SoftwareCenterError::Repository)?;
        let entry_count = self.count_entries().await?;
        let age_seconds = compute_age_seconds(&activated_at);
        let last_refresh = self.last_refresh().await?;
        let stale = age_seconds > self.staleness_threshold;
        Ok(CatalogDiagnostics {
            source_url,
            source_id: if embedded != 0 {
                "embedded".to_owned()
            } else {
                "http".to_owned()
            },
            manifest_digest,
            entry_count,
            activated_at,
            age_seconds,
            last_refresh,
            staleness_threshold_seconds: self.staleness_threshold,
            stale,
        })
    }

    /// Record a refresh attempt outcome.
    pub async fn record_refresh(
        &self,
        source_url: &str,
        outcome: &str,
        manifest_digest: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        sqlx::query(
            "INSERT INTO software_refresh_history(source_url, attempted_at, outcome, manifest_digest, error) VALUES(?,?,?,?,?)",
        )
        .bind(source_url)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(outcome)
        .bind(manifest_digest)
        .bind(error)
        .execute(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?;
        Ok(())
    }

    async fn tags_for(&self, id: &str) -> Result<Vec<String>, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let rows =
            sqlx::query("SELECT tag FROM software_entry_tags WHERE entry_id = ? ORDER BY tag")
                .bind(id)
                .fetch_all(pool)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
        rows.into_iter()
            .map(|row| {
                row.try_get::<String, _>("tag")
                    .map_err(|_| SoftwareCenterError::Repository)
            })
            .collect()
    }

    async fn deps_for(&self, id: &str) -> Result<Vec<String>, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let rows = sqlx::query(
            "SELECT dep_id FROM software_entry_deps WHERE entry_id = ? ORDER BY dep_id",
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?;
        rows.into_iter()
            .map(|row| {
                row.try_get::<String, _>("dep_id")
                    .map_err(|_| SoftwareCenterError::Repository)
            })
            .collect()
    }

    async fn conflicts_for(&self, id: &str) -> Result<Vec<String>, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let rows = sqlx::query(
            "SELECT conflict_id FROM software_entry_conflicts WHERE entry_id = ? ORDER BY conflict_id",
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?;
        rows.into_iter()
            .map(|row| {
                row.try_get::<String, _>("conflict_id")
                    .map_err(|_| SoftwareCenterError::Repository)
            })
            .collect()
    }

    async fn versions_for(&self, id: &str) -> Result<Vec<StorefrontVersion>, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let rows = sqlx::query(
            "SELECT version, size_bytes, changelog_url, supports_php_json, released_at, is_latest FROM software_entry_versions WHERE entry_id = ? ORDER BY is_latest DESC, version DESC",
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let supports_php: Vec<String> = serde_json::from_str(
                row.try_get::<String, _>("supports_php_json")
                    .map_err(|_| SoftwareCenterError::Repository)?
                    .as_str(),
            )
            .map_err(|_| SoftwareCenterError::Repository)?;
            out.push(StorefrontVersion {
                version: row
                    .try_get("version")
                    .map_err(|_| SoftwareCenterError::Repository)?,
                size_bytes: row
                    .try_get::<i64, _>("size_bytes")
                    .map_err(|_| SoftwareCenterError::Repository)?
                    as u64,
                supports_php,
                released_at: row
                    .try_get::<Option<String>, _>("released_at")
                    .map_err(|_| SoftwareCenterError::Repository)?,
                changelog_url: row
                    .try_get::<Option<String>, _>("changelog_url")
                    .map_err(|_| SoftwareCenterError::Repository)?,
                is_latest: row
                    .try_get::<i64, _>("is_latest")
                    .map_err(|_| SoftwareCenterError::Repository)?
                    != 0,
            });
        }
        Ok(out)
    }

    async fn install_state_for(&self, id: &str) -> Result<String, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let row = sqlx::query("SELECT managed FROM software_components WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        if let Some(row) = row {
            let managed: i64 = row
                .try_get("managed")
                .map_err(|_| SoftwareCenterError::Repository)?;
            return Ok(if managed != 0 {
                "panel_managed".to_owned()
            } else {
                "externally_managed".to_owned()
            });
        }
        let entry_row = sqlx::query("SELECT id FROM software_entries WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        Ok(if entry_row.is_some() {
            "available".to_owned()
        } else {
            "unsupported".to_owned()
        })
    }

    async fn installed_version_for(&self, id: &str) -> Result<Option<String>, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let row = sqlx::query("SELECT version FROM software_components WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        if let Some(row) = row {
            return row
                .try_get::<Option<String>, _>("version")
                .map_err(|_| SoftwareCenterError::Repository);
        }
        Ok(None)
    }

    async fn count_entries(&self) -> Result<usize, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM software_entries")
            .fetch_one(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        Ok(count.max(0) as usize)
    }

    async fn last_refresh(&self) -> Result<Option<LastRefresh>, SoftwareCenterError> {
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let row = sqlx::query(
            "SELECT source_url, attempted_at, outcome, manifest_digest, error FROM software_refresh_history ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?;
        Ok(row.map(|row| LastRefresh {
            attempted_at: row
                .try_get("attempted_at")
                .map_err(|_| SoftwareCenterError::Repository)
                .unwrap_or_default(),
            outcome: row
                .try_get("outcome")
                .map_err(|_| SoftwareCenterError::Repository)
                .unwrap_or_default(),
            manifest_digest: row
                .try_get::<Option<String>, _>("manifest_digest")
                .map_err(|_| SoftwareCenterError::Repository)
                .ok()
                .flatten(),
            error: row
                .try_get::<Option<String>, _>("error")
                .map_err(|_| SoftwareCenterError::Repository)
                .ok()
                .flatten(),
        }))
    }
}

/// Host context used for pre-flight compatibility checks.
#[derive(Debug, Clone, Default)]
pub struct CompatibilityHost {
    pub platform: Option<String>,
    pub architecture: Option<String>,
    pub php_version: Option<String>,
    pub installed_packages: Vec<String>,
}

fn build_search_text(entry: &CatalogEntryRecipe) -> String {
    let tags = entry
        .tags
        .iter()
        .map(|tag| tag.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{} {} {} {} {} {}",
        entry.name.to_ascii_lowercase(),
        entry.description.to_ascii_lowercase(),
        entry.developer.to_ascii_lowercase(),
        entry.license.as_str().to_ascii_lowercase(),
        entry.category.slug(),
        tags.to_ascii_lowercase()
    )
}

fn parse_category(slug: &str) -> Result<Category, SoftwareCenterError> {
    slug.parse::<Category>()
        .map_err(|_| SoftwareCenterError::Invalid)
}

fn parse_kind(value: &str) -> Result<EntryKind, SoftwareCenterError> {
    match value {
        "system" => Ok(EntryKind::System),
        "web" => Ok(EntryKind::Web),
        "tool" => Ok(EntryKind::Tool),
        _ => Err(SoftwareCenterError::Invalid),
    }
}

fn sort_label(sort: CatalogSort) -> &'static str {
    match sort {
        CatalogSort::Name => "name",
        CatalogSort::Recent => "recent",
        CatalogSort::Size => "size",
    }
}

// Re-export the sort enum from the query module to keep the call site
// consistent with the public domain API.
use openpanel_domain::software_center::CatalogSort;

fn now_unix() -> Result<u64, SoftwareCenterError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| SoftwareCenterError::Repository)
}

fn compute_age_seconds(activated_at: &str) -> i64 {
    let Ok(activated) = chrono::DateTime::parse_from_rfc3339(activated_at) else {
        return i64::MAX;
    };
    let now = chrono::Utc::now();
    (now - activated.with_timezone(&chrono::Utc)).num_seconds()
}

impl From<RecipeError> for SoftwareCenterError {
    fn from(_: RecipeError) -> Self {
        SoftwareCenterError::Invalid
    }
}

impl From<serde_json::Error> for SoftwareCenterError {
    fn from(_: serde_json::Error) -> Self {
        SoftwareCenterError::Repository
    }
}

// `BTreeMap` is imported above for potential future use; reference it
// to silence the unused-import warning while keeping the import for the
// public surface.
#[allow(dead_code)]
fn _btreemap_marker() -> BTreeMap<String, String> {
    BTreeMap::new()
}

// `CatalogManifest` is imported above for the embedded seed path; reference
// it to keep the import live.
#[allow(dead_code)]
const _MANIFEST_TYPE: fn() -> Option<CatalogManifest> = || None;
