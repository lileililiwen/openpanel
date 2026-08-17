//! Site cache and CDN integration bounded context: the typed
//! per-site `SiteCachePolicy`, the `CdnIntegration` aggregate with
//! its `CdnAdapter` contract, and the purge / cache-level operations
//! that let operators attach a page-cache layer and a CDN to a site.
//!
//! I/O-free: the adapter trait and repository port are declared here;
//! concrete nginx snippet generation, HTTP adapters, and SQLite
//! persistence live in the application layer.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Default page-cache TTL in seconds (design: 60s).
pub const DEFAULT_TTL_SECONDS: u32 = 60;
/// Default static-asset cache TTL in seconds (design: 7 days).
pub const DEFAULT_STATIC_ASSETS_TTL_SECONDS: u32 = 7 * 24 * 60 * 60;
/// Maximum number of bypass paths on a policy.
pub const MAX_BYPASS_PATHS: usize = 64;
/// Maximum number of keyed cookies on a policy.
pub const MAX_KEYED_COOKIES: usize = 16;

/// Per-site page-cache policy. Drives both the nginx microcache
/// snippet and (optionally) the CDN cache level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteCachePolicy {
    site_id: Uuid,
    ttl_seconds: u32,
    bypass_paths: Vec<String>,
    static_assets_ttl_seconds: u32,
    keyed_cookies: Vec<String>,
    stale_while_revalidate: bool,
    revalidation_required: bool,
}

impl SiteCachePolicy {
    /// Build a policy with defaults (`ttl=60s`, static assets `7d`).
    pub fn new(site_id: Uuid) -> Result<Self, SiteCacheCdnError> {
        Self::with_ttl(site_id, DEFAULT_TTL_SECONDS)
    }

    /// Build a policy with an explicit page TTL. The TTL must be
    /// `1..=31536000` (one year).
    pub fn with_ttl(site_id: Uuid, ttl_seconds: u32) -> Result<Self, SiteCacheCdnError> {
        if ttl_seconds == 0 || ttl_seconds > 31_536_000 {
            return Err(SiteCacheCdnError::InvalidTtl(ttl_seconds));
        }
        Ok(Self {
            site_id,
            ttl_seconds,
            bypass_paths: Vec::new(),
            static_assets_ttl_seconds: DEFAULT_STATIC_ASSETS_TTL_SECONDS,
            keyed_cookies: Vec::new(),
            stale_while_revalidate: false,
            revalidation_required: true,
        })
    }

    /// The site this policy governs.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Page-cache TTL in seconds.
    pub fn ttl_seconds(&self) -> u32 {
        self.ttl_seconds
    }

    /// Paths that bypass the cache entirely.
    pub fn bypass_paths(&self) -> &[String] {
        &self.bypass_paths
    }

    /// TTL for static assets (images, CSS, JS) in seconds.
    pub fn static_assets_ttl_seconds(&self) -> u32 {
        self.static_assets_ttl_seconds
    }

    /// Cookies that MUST NOT be part of the cache key.
    pub fn keyed_cookies(&self) -> &[String] {
        &self.keyed_cookies
    }

    /// Serve stale while revalidating in the background.
    pub fn stale_while_revalidate(&self) -> bool {
        self.stale_while_revalidate
    }

    /// Whether upstream revalidation is required before serving a
    /// cached response.
    pub fn revalidation_required(&self) -> bool {
        self.revalidation_required
    }

    /// Set the bypass path list. Each path must be absolute
    /// (start with `/`); `*` globs are allowed. Duplicates are
    /// rejected.
    pub fn with_bypass_paths(mut self, paths: Vec<String>) -> Result<Self, SiteCacheCdnError> {
        if paths.len() > MAX_BYPASS_PATHS {
            return Err(SiteCacheCdnError::TooManyBypassPaths(paths.len()));
        }
        let mut seen = std::collections::HashSet::new();
        for p in &paths {
            if !p.starts_with('/') {
                return Err(SiteCacheCdnError::InvalidBypassPath(p.clone()));
            }
            if !seen.insert(p.clone()) {
                return Err(SiteCacheCdnError::InvalidBypassPath(format!(
                    "duplicate path `{p}`"
                )));
            }
        }
        self.bypass_paths = paths;
        Ok(self)
    }

    /// Set the keyed-cookie list (cookies excluded from the cache
    /// key). Names must match `[a-zA-Z0-9_]+`.
    pub fn with_keyed_cookies(mut self, cookies: Vec<String>) -> Result<Self, SiteCacheCdnError> {
        if cookies.len() > MAX_KEYED_COOKIES {
            return Err(SiteCacheCdnError::TooManyKeyedCookies(cookies.len()));
        }
        for c in &cookies {
            if c.is_empty() || !c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
                return Err(SiteCacheCdnError::InvalidKeyedCookie(c.clone()));
            }
        }
        self.keyed_cookies = cookies;
        Ok(self)
    }

    /// Set the static-asset TTL. Must be `1..=31536000`.
    pub fn with_static_assets_ttl(mut self, seconds: u32) -> Result<Self, SiteCacheCdnError> {
        if seconds == 0 || seconds > 31_536_000 {
            return Err(SiteCacheCdnError::InvalidTtl(seconds));
        }
        self.static_assets_ttl_seconds = seconds;
        Ok(self)
    }

    /// Enable or disable stale-while-revalidate.
    pub fn with_stale_while_revalidate(mut self, enabled: bool) -> Self {
        self.stale_while_revalidate = enabled;
        self
    }

    /// Enable or disable revalidation-required.
    pub fn with_revalidation_required(mut self, required: bool) -> Self {
        self.revalidation_required = required;
        self
    }

    /// Whether the given request path bypasses the cache.
    pub fn bypasses(&self, path: &str) -> bool {
        self.bypass_paths.iter().any(|p| glob_match(p, path))
    }
}

/// Simple glob matcher supporting a single trailing `*` wildcard
/// (e.g. `/wp-admin/*`). Matches the full path.
fn glob_match(pattern: &str, path: &str) -> bool {
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        path.starts_with(prefix) && path.ends_with(suffix)
    } else {
        path == pattern
    }
}

/// CDN providers an integration can target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdnKind {
    /// Cloudflare REST API.
    Cloudflare,
    /// AWS CloudFront invalidation API.
    CloudFront,
    /// Generic HTTP webhook.
    GenericHttp,
}

impl CdnKind {
    /// Wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            CdnKind::Cloudflare => "cloudflare",
            CdnKind::CloudFront => "cloudfront",
            CdnKind::GenericHttp => "generic_http",
        }
    }
}

/// Cache level applied on the CDN edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheLevel {
    /// No caching on the edge.
    Off,
    /// Basic caching (HTML served stale immediately).
    Basic,
    /// Standard caching (respect upstream cache headers).
    Standard,
    /// Aggressive caching (ignore upstream, cache everything).
    Aggressive,
}

impl CacheLevel {
    /// Wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheLevel::Off => "off",
            CacheLevel::Basic => "basic",
            CacheLevel::Standard => "standard",
            CacheLevel::Aggressive => "aggressive",
        }
    }
}

/// A CDN zone reference: the CDN-side site identifier plus a
/// human label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdnZone {
    id: String,
    name: String,
}

impl CdnZone {
    /// Build a zone ref.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    /// CDN-side zone identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Zone display name (often the domain).
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A stored CDN integration: how the panel talks to a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdnIntegration {
    id: Uuid,
    name: String,
    kind: CdnKind,
    /// Encrypted credentials / endpoint config (opaque; never
    /// echoed). Format is provider-specific and managed by the
    /// adapter.
    config_enc: String,
    enabled: bool,
    created_at: DateTime<Utc>,
}

impl CdnIntegration {
    /// Build an integration. The name must be 1..=64 chars and the
    /// encrypted config must be non-empty.
    pub fn new(
        id: Uuid,
        name: impl Into<String>,
        kind: CdnKind,
        config_enc: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, SiteCacheCdnError> {
        let name = name.into();
        if name.is_empty() || name.len() > 64 {
            return Err(SiteCacheCdnError::InvalidName(format!(
                "name must be 1..=64 chars, got {}",
                name.len()
            )));
        }
        let config_enc = config_enc.into();
        if config_enc.is_empty() {
            return Err(SiteCacheCdnError::InvalidConfig(
                "encrypted config must not be empty".to_string(),
            ));
        }
        Ok(Self {
            id,
            name,
            kind,
            config_enc,
            enabled: true,
            created_at,
        })
    }

    /// Identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Provider kind.
    pub fn kind(&self) -> CdnKind {
        self.kind
    }

    /// The encrypted provider config.
    pub fn config_enc(&self) -> &str {
        &self.config_enc
    }

    /// Whether the integration is enabled.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Creation timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Enable or disable the integration.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

/// A single purge operation: which CDN (or site-cache zone) to
/// purge and which paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeRequest {
    integration_id: Uuid,
    paths: Vec<String>,
}

impl PurgeRequest {
    /// Build a purge request. Paths must be absolute and non-empty.
    pub fn new(integration_id: Uuid, paths: Vec<String>) -> Result<Self, SiteCacheCdnError> {
        if paths.is_empty() {
            return Err(SiteCacheCdnError::EmptyPurge);
        }
        if paths.len() > 100 {
            return Err(SiteCacheCdnError::TooManyPurgePaths(paths.len()));
        }
        for p in &paths {
            if !p.starts_with('/') {
                return Err(SiteCacheCdnError::InvalidPurgePath(p.clone()));
            }
        }
        Ok(Self {
            integration_id,
            paths,
        })
    }

    /// Integration to purge through.
    pub fn integration_id(&self) -> Uuid {
        self.integration_id
    }

    /// Paths to purge.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }
}

/// Receipt returned by a successful purge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeReceipt {
    purged: Vec<String>,
    at: DateTime<Utc>,
}

impl PurgeReceipt {
    /// Build a receipt.
    pub fn new(purged: Vec<String>, at: DateTime<Utc>) -> Self {
        Self { purged, at }
    }

    /// Paths the CDN acknowledged.
    pub fn purged(&self) -> &[String] {
        &self.purged
    }

    /// When the purge completed.
    pub fn at(&self) -> DateTime<Utc> {
        self.at
    }
}

/// Cache-related response headers summarized by `get_headers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderSummary {
    cache_control: Option<String>,
    age_seconds: Option<u64>,
    cf_cache_status: Option<String>,
}

impl HeaderSummary {
    /// Build a header summary.
    pub fn new(
        cache_control: Option<String>,
        age_seconds: Option<u64>,
        cf_cache_status: Option<String>,
    ) -> Self {
        Self {
            cache_control,
            age_seconds,
            cf_cache_status,
        }
    }

    /// The `Cache-Control` header value, if present.
    pub fn cache_control(&self) -> Option<&str> {
        self.cache_control.as_deref()
    }

    /// The `Age` header in seconds, if present.
    pub fn age_seconds(&self) -> Option<u64> {
        self.age_seconds
    }

    /// The `CF-Cache-Status` header, if present.
    pub fn cf_cache_status(&self) -> Option<&str> {
        self.cf_cache_status.as_deref()
    }
}

/// CDN adapter contract. Concrete providers (Cloudflare,
/// CloudFront, generic webhook) implement this against their APIs.
#[async_trait]
pub trait CdnAdapter: Send + Sync + 'static {
    /// Type-erased error.
    type Error: std::error::Error + Send + Sync + 'static;

    /// List the zones the integration can address.
    async fn list_zones(&self) -> Result<Vec<CdnZone>, Self::Error>;
    /// Purge the given paths for a zone. Returns a receipt.
    async fn purge(&self, zone: &CdnZone, paths: &[String]) -> Result<PurgeReceipt, Self::Error>;
    /// Set the cache level for a zone.
    async fn set_cache_level(&self, zone: &CdnZone, level: CacheLevel) -> Result<(), Self::Error>;
    /// Fetch cache-related response headers for a zone.
    async fn get_headers(&self, zone: &CdnZone) -> Result<HeaderSummary, Self::Error>;
}

/// Persistence port for the site-cache-cdn bounded context.
#[async_trait]
pub trait SiteCacheCdnRepository: Send + Sync + 'static {
    /// Store (or replace) a site's cache policy.
    async fn upsert_policy(&self, policy: &SiteCachePolicy) -> Result<(), SiteCacheCdnError>;
    /// Load a site's cache policy, if any.
    async fn policy_for_site(
        &self,
        site_id: Uuid,
    ) -> Result<Option<SiteCachePolicy>, SiteCacheCdnError>;
    /// Insert a CDN integration.
    async fn insert_integration(
        &self,
        integration: &CdnIntegration,
    ) -> Result<(), SiteCacheCdnError>;
    /// Find a CDN integration by id.
    async fn find_integration(&self, id: Uuid)
    -> Result<Option<CdnIntegration>, SiteCacheCdnError>;
    /// List all CDN integrations.
    async fn list_integrations(&self) -> Result<Vec<CdnIntegration>, SiteCacheCdnError>;
    /// Delete a CDN integration by id. Missing ids are an error.
    async fn delete_integration(&self, id: Uuid) -> Result<(), SiteCacheCdnError>;
    /// Append a purge to the purge log.
    async fn record_purge(
        &self,
        integration_id: Uuid,
        receipt: &PurgeReceipt,
    ) -> Result<(), SiteCacheCdnError>;
    /// Recent purge log entries for an integration (most recent first).
    async fn recent_purges(
        &self,
        integration_id: Uuid,
    ) -> Result<Vec<PurgeReceipt>, SiteCacheCdnError>;
}

/// Errors that can occur in the site-cache-cdn bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SiteCacheCdnError {
    /// The TTL is out of range.
    #[error("invalid cache TTL: {0}")]
    InvalidTtl(u32),
    /// Too many bypass paths.
    #[error("too many bypass paths: {0}")]
    TooManyBypassPaths(usize),
    /// A bypass path is malformed.
    #[error("invalid bypass path: {0}")]
    InvalidBypassPath(String),
    /// Too many keyed cookies.
    #[error("too many keyed cookies: {0}")]
    TooManyKeyedCookies(usize),
    /// A keyed-cookie name is malformed.
    #[error("invalid keyed cookie: {0}")]
    InvalidKeyedCookie(String),
    /// The integration name is invalid.
    #[error("invalid CDN integration name: {0}")]
    InvalidName(String),
    /// The encrypted config is malformed.
    #[error("invalid CDN integration config: {0}")]
    InvalidConfig(String),
    /// A purge request had no paths.
    #[error("purge request must include at least one path")]
    EmptyPurge,
    /// A purge request had too many paths.
    #[error("too many purge paths: {0}")]
    TooManyPurgePaths(usize),
    /// A purge path is not absolute.
    #[error("invalid purge path: {0}")]
    InvalidPurgePath(String),
    /// The CDN integration was not found.
    #[error("CDN integration not found")]
    IntegrationNotFound,
    /// The site cache policy was not found.
    #[error("cache policy not found")]
    PolicyNotFound,
    /// The CDN adapter reported an error.
    #[error("CDN adapter error: {0}")]
    Adapter(String),
    /// No adapter is compiled into the binary for the requested kind.
    #[error("no CDN adapter available for kind `{0}`")]
    AdapterNotAvailable(String),
    /// Decrypting an encrypted CDN credential failed (tampered or wrong key).
    #[error("CDN credential decryption failed: {0}")]
    CryptoFailure(String),
    /// Persistence layer failure.
    #[error("site-cache-cdn persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for SiteCacheCdnError {
    fn from(error: RepoError) -> Self {
        SiteCacheCdnError::Persistence(error.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_defaults() {
        let policy = SiteCachePolicy::new(Uuid::new_v4()).unwrap();
        assert_eq!(policy.ttl_seconds(), DEFAULT_TTL_SECONDS);
        assert_eq!(
            policy.static_assets_ttl_seconds(),
            DEFAULT_STATIC_ASSETS_TTL_SECONDS
        );
        assert!(policy.bypass_paths().is_empty());
        assert!(policy.revalidation_required());
    }

    #[test]
    fn policy_rejects_zero_ttl() {
        let err = SiteCachePolicy::with_ttl(Uuid::new_v4(), 0).expect_err("must reject");
        assert!(matches!(err, SiteCacheCdnError::InvalidTtl(0)));
    }

    #[test]
    fn bypass_glob_matching() {
        let policy = SiteCachePolicy::new(Uuid::new_v4())
            .unwrap()
            .with_bypass_paths(vec!["/wp-admin/*".to_string(), "/api/login".to_string()])
            .unwrap();
        assert!(policy.bypasses("/wp-admin/index.php"));
        assert!(policy.bypasses("/api/login"));
        assert!(!policy.bypasses("/api/data"));
        assert!(!policy.bypasses("/articles"));
    }

    #[test]
    fn bypass_path_must_be_absolute() {
        let err = SiteCachePolicy::new(Uuid::new_v4())
            .unwrap()
            .with_bypass_paths(vec!["wp-admin".to_string()])
            .expect_err("must reject");
        assert!(matches!(err, SiteCacheCdnError::InvalidBypassPath(_)));
    }

    #[test]
    fn keyed_cookies_reject_invalid_names() {
        let err = SiteCachePolicy::new(Uuid::new_v4())
            .unwrap()
            .with_keyed_cookies(vec!["bad cookie!".to_string()])
            .expect_err("must reject");
        assert!(matches!(err, SiteCacheCdnError::InvalidKeyedCookie(_)));
    }

    #[test]
    fn purge_request_requires_absolute_paths() {
        let err = PurgeRequest::new(Uuid::new_v4(), vec!["relative".to_string()])
            .expect_err("must reject");
        assert!(matches!(err, SiteCacheCdnError::InvalidPurgePath(_)));
        let err = PurgeRequest::new(Uuid::new_v4(), Vec::new()).expect_err("must reject");
        assert!(matches!(err, SiteCacheCdnError::EmptyPurge));
    }

    #[test]
    fn integration_roundtrip_fields() {
        let integration = CdnIntegration::new(
            Uuid::new_v4(),
            "prod-cdn",
            CdnKind::Cloudflare,
            "aabb:ccdd",
            Utc::now(),
        )
        .unwrap();
        assert_eq!(integration.kind(), CdnKind::Cloudflare);
        assert!(integration.enabled());
        let mut integration = integration;
        integration.set_enabled(false);
        assert!(!integration.enabled());
    }

    #[test]
    fn cdn_kind_wire_names() {
        assert_eq!(CdnKind::Cloudflare.as_str(), "cloudflare");
        assert_eq!(CdnKind::CloudFront.as_str(), "cloudfront");
        assert_eq!(CdnKind::GenericHttp.as_str(), "generic_http");
        assert_eq!(CacheLevel::Aggressive.as_str(), "aggressive");
    }
}
