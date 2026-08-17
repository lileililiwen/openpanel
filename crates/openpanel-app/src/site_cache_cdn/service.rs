//! Site cache and CDN application service: cache policy lifecycle,
//! CDN integration management, nginx snippet application with
//! rollback, and purge orchestration through registered adapters.

use std::{path::PathBuf, sync::Arc};

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    CacheLevel, CdnAdapter, CdnIntegration, CdnKind, CdnZone, PurgeReceipt, PurgeRequest,
    SiteCacheCdnError, SiteCacheCdnRepository, SiteCachePolicy,
};
use uuid::Uuid;

use crate::site_cache_cdn::{
    GenericHttpAdapter,
    adapters::{CdnProviderConfig, CloudFrontAdapter, CloudflareAdapter, provider_config_for},
    nginx_apply::{NginxApplyError, NginxCacheManager},
    nginx_cache::cache_directives,
};

/// Default `proxy_cache_path` max size (MB) used when rendering a
/// site's cache snippet.
const DEFAULT_MAX_SIZE_MB: u64 = 512;

/// Summary of a CDN purge operation across one or more paths.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PurgeSummary {
    /// Number of paths the upstream acknowledged.
    pub successful: usize,
    /// Number of paths that failed upstream.
    pub failed: usize,
    /// Redacted per-path failure reasons (`path=.. reason=..`). No
    /// secrets or tokens are included.
    pub redacted: Vec<String>,
}

impl PurgeSummary {
    /// Build a summary.
    pub fn new(successful: usize, failed: usize, redacted: Vec<String>) -> Self {
        Self {
            successful,
            failed,
            redacted,
        }
    }
}

/// Application service for site cache policies and CDN integrations.
#[derive(Clone)]
pub struct SiteCacheService {
    repo: Arc<dyn SiteCacheCdnRepository>,
    audit: Arc<dyn AuditService>,
    registry: Arc<CdnAdapterRegistry>,
}

impl SiteCacheService {
    /// Build a service over a repository, audit sink, and adapter
    /// registry. Credentials stored via `create_integration` are
    /// recorded opaquely; the panel relies on the bounded-context's
    /// typed encryption adapter when a caller wants AES-256-GCM
    /// wrapping (`encrypt_cdn_secret` / `decrypt_cdn_secret`).
    pub fn new(
        repo: Arc<dyn SiteCacheCdnRepository>,
        audit: Arc<dyn AuditService>,
        registry: Arc<CdnAdapterRegistry>,
    ) -> Self {
        Self {
            repo,
            audit,
            registry,
        }
    }

    /// Return the cache policy for a site, or the default when the
    /// site has none stored.
    pub async fn cache_policy(&self, site_id: Uuid) -> Result<SiteCachePolicy, SiteCacheCdnError> {
        Ok(self
            .repo
            .policy_for_site(site_id)
            .await?
            .unwrap_or(SiteCachePolicy::new(site_id)?))
    }

    /// Persist a site's cache policy and audit `SiteCachePolicyUpdated`
    /// carrying only the site id and TTL.
    pub async fn set_cache_policy(
        &self,
        actor: &str,
        policy: &SiteCachePolicy,
    ) -> Result<(), SiteCacheCdnError> {
        self.repo.upsert_policy(policy).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::SiteCachePolicyUpdated,
                    AuditOutcome::Success,
                )
                .target(policy.site_id().to_string())
                .metadata(serde_json::json!({
                    "ttl_seconds": policy.ttl_seconds(),
                    "bypass_paths": policy.bypass_paths().len(),
                })),
            )
            .await
            .ok();
        Ok(())
    }

    /// Persist a policy and render + apply its nginx snippet under
    /// `include_root`. On a successful `nginx -t` + reload, audits
    /// `SiteCachePolicyApplied`; if the test fails, the prior snippet
    /// is restored and `SiteCachePolicyRolledBack` is audited.
    pub async fn set_cache_policy_and_apply(
        &self,
        actor: &str,
        policy: &SiteCachePolicy,
        include_root: &std::path::Path,
    ) -> Result<(), SiteCacheCdnError> {
        let site_id = policy.site_id();
        self.repo.upsert_policy(policy).await?;
        let snippet = cache_directives(policy, DEFAULT_MAX_SIZE_MB);
        let manager = NginxCacheManager::new(include_root.to_path_buf());
        match manager.apply(site_id, &snippet) {
            Ok(_) => {
                self.audit
                    .record(
                        AuditEvent::new(
                            actor,
                            AuditAction::SiteCachePolicyApplied,
                            AuditOutcome::Success,
                        )
                        .target(site_id.to_string())
                        .metadata(serde_json::json!({
                            "ttl_seconds": policy.ttl_seconds(),
                        })),
                    )
                    .await
                    .ok();
                Ok(())
            }
            Err(e) => {
                self.audit
                    .record(
                        AuditEvent::new(
                            actor,
                            AuditAction::SiteCachePolicyRolledBack,
                            AuditOutcome::Failure,
                        )
                        .target(site_id.to_string())
                        .metadata(serde_json::json!({ "reason": format!("{e:?}") })),
                    )
                    .await
                    .ok();
                Err(map_nginx_error(e))
            }
        }
    }

    /// Create a CDN integration. The `config_opaque` payload is
    /// stored as-is (callers wanting AES-256-GCM encryption should
    /// wrap it before calling). Rejects kinds for which the binary
    /// has no compiled adapter.
    pub async fn create_integration(
        &self,
        actor: &str,
        name: &str,
        kind: CdnKind,
        config_opaque: &str,
    ) -> Result<CdnIntegration, SiteCacheCdnError> {
        if self.registry.get(kind).is_err() {
            return Err(SiteCacheCdnError::AdapterNotAvailable(
                kind.as_str().to_string(),
            ));
        }
        let integration = CdnIntegration::new(
            Uuid::new_v4(),
            name,
            kind,
            config_opaque,
            chrono::Utc::now(),
        )?;
        self.repo.insert_integration(&integration).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::CdnIntegrationCreated,
                    AuditOutcome::Success,
                )
                .target(integration.id().to_string())
                .metadata(serde_json::json!({ "kind": kind.as_str() })),
            )
            .await
            .ok();
        Ok(integration)
    }

    /// Create a CDN integration with the per-provider fields. The
    /// config is JSON-encoded then stored opaquely. Rejects kinds
    /// for which the binary has no compiled adapter.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_integration_for_provider(
        &self,
        actor: &str,
        name: &str,
        kind: CdnKind,
        api_token: Option<String>,
        zone_id: Option<String>,
        webhook_url: Option<String>,
        region: Option<String>,
        secret_key: Option<String>,
    ) -> Result<CdnIntegration, SiteCacheCdnError> {
        if self.registry.get(kind).is_err() {
            return Err(SiteCacheCdnError::AdapterNotAvailable(
                kind.as_str().to_string(),
            ));
        }
        let config = provider_config_for(kind, api_token, zone_id, webhook_url, region, secret_key);
        let config_opaque = config.to_json()?;
        let integration = CdnIntegration::new(
            Uuid::new_v4(),
            name,
            kind,
            config_opaque,
            chrono::Utc::now(),
        )?;
        self.repo.insert_integration(&integration).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::CdnIntegrationCreated,
                    AuditOutcome::Success,
                )
                .target(integration.id().to_string())
                .metadata(serde_json::json!({ "kind": kind.as_str() })),
            )
            .await
            .ok();
        Ok(integration)
    }

    /// List CDN integrations. The stored config is never echoed in
    /// a structured way — callers reading the opaque blob are
    /// responsible for treating it as a secret.
    pub async fn list_integrations(&self) -> Result<Vec<CdnIntegration>, SiteCacheCdnError> {
        self.repo.list_integrations().await
    }

    /// Delete a CDN integration by id.
    pub async fn delete_integration(&self, actor: &str, id: Uuid) -> Result<(), SiteCacheCdnError> {
        self.repo.delete_integration(id).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::CdnIntegrationDeleted,
                    AuditOutcome::Success,
                )
                .target(id.to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Purge a list of paths through an integration's adapter.
    /// Returns a summary; partial failures are recorded in
    /// `CdnPurgePartial` and surfaced in the redacted list.
    pub async fn purge_via_integration(
        &self,
        actor: &str,
        integration_id: Uuid,
        paths: Vec<String>,
    ) -> Result<PurgeSummary, SiteCacheCdnError> {
        let integration = self
            .repo
            .find_integration(integration_id)
            .await?
            .ok_or(SiteCacheCdnError::IntegrationNotFound)?;

        let config_json = integration.config_enc().to_string();
        let config = match CdnProviderConfig::from_json(&config_json) {
            Ok(c) => c,
            Err(_) => {
                self.audit
                    .record(
                        AuditEvent::new(
                            actor,
                            AuditAction::CdnCredentialDecryptFailed,
                            AuditOutcome::Failure,
                        )
                        .target(integration.id().to_string()),
                    )
                    .await
                    .ok();
                return Err(SiteCacheCdnError::CryptoFailure(
                    "stored CDN credential could not be parsed".to_string(),
                ));
            }
        };
        let adapter: Box<dyn CdnAdapter<Error = SiteCacheCdnError>> =
            match self.registry.get(integration.kind()) {
                Ok(_) => Box::new(RegisteredAdapter::new(
                    self.registry.clone(),
                    integration.kind(),
                )),
                Err(_) => build_adapter(integration.kind(), &config),
            };
        let zone = CdnZone::new(
            config
                .zone_id
                .clone()
                .unwrap_or_else(|| integration.id().to_string()),
            integration.name().to_string(),
        );

        let (successful, failed, redacted) = collect_purge(&*adapter, &zone, &paths).await;

        if failed > 0 {
            self.audit
                .record(
                    AuditEvent::new(actor, AuditAction::CdnPurgePartial, AuditOutcome::Failure)
                        .target(integration.id().to_string())
                        .metadata(serde_json::json!({
                            "successful": successful.len(),
                            "failed": failed,
                        })),
                )
                .await
                .ok();
        }

        if !successful.is_empty() {
            let receipt = PurgeReceipt::new(successful.clone(), chrono::Utc::now());
            self.repo.record_purge(integration.id(), &receipt).await?;
            self.audit
                .record(
                    AuditEvent::new(actor, AuditAction::CdnPurged, AuditOutcome::Success)
                        .target(integration.id().to_string())
                        .metadata(serde_json::json!({
                            "zone": zone.id(),
                            "purged": receipt.purged().len(),
                        })),
                )
                .await
                .ok();
        }

        Ok(PurgeSummary::new(successful.len(), failed, redacted))
    }

    /// Purge a list of paths from a site's local nginx cache. Best
    /// effort: removes cache files whose path matches under the site's
    /// cache root. Idempotent — a second call removes nothing and is
    /// not re-audited.
    pub async fn purge_site_cache(
        &self,
        actor: &str,
        site_id: Uuid,
        paths: Vec<String>,
    ) -> Result<PurgeReceipt, SiteCacheCdnError> {
        let request = PurgeRequest::new(site_id, paths)?;
        let cache_root = PathBuf::from("/var/cache/openpanel").join(site_id.to_string());
        let mut removed = Vec::new();
        if cache_root.exists() {
            for path in request.paths() {
                let target = cache_root.join(path.trim_start_matches('/'));
                if target.exists() && std::fs::remove_file(&target).is_ok() {
                    removed.push(path.clone());
                }
            }
        }
        if !removed.is_empty() {
            self.audit
                .record(
                    AuditEvent::new(actor, AuditAction::CdnPurged, AuditOutcome::Success)
                        .target(site_id.to_string())
                        .metadata(serde_json::json!({ "purged": removed.len() })),
                )
                .await
                .ok();
        }
        Ok(PurgeReceipt::new(removed, chrono::Utc::now()))
    }

    /// The adapter registry handle.
    pub fn registry(&self) -> Arc<CdnAdapterRegistry> {
        self.registry.clone()
    }

    /// The most recent purge receipts recorded for an integration,
    /// newest first (up to 50).
    pub async fn recent_purges(
        &self,
        integration_id: Uuid,
    ) -> Result<Vec<PurgeReceipt>, SiteCacheCdnError> {
        self.repo.recent_purges(integration_id).await
    }

    /// Purge paths through an adapter (used by tests and the generic
    /// integration path). Records the receipt and audits `CdnPurged`.
    pub async fn purge<A>(
        &self,
        actor: &str,
        adapter: &A,
        zone: &CdnZone,
        request: &PurgeRequest,
    ) -> Result<PurgeReceipt, SiteCacheCdnError>
    where
        A: CdnAdapter<Error = SiteCacheCdnError> + ?Sized,
    {
        let receipt = adapter
            .purge(zone, request.paths())
            .await
            .map_err(|e| SiteCacheCdnError::Adapter(e.to_string()))?;
        self.repo
            .record_purge(request.integration_id(), &receipt)
            .await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::CdnPurged, AuditOutcome::Success)
                    .target(request.integration_id().to_string())
                    .metadata(serde_json::json!({
                        "zone": zone.id(),
                        "purged": receipt.purged().len(),
                    })),
            )
            .await
            .ok();
        Ok(receipt)
    }
}

/// Adapter wrapper that forwards every call through the registry,
/// honouring whatever concrete adapter is currently registered for a
/// kind (so tests can swap in a MemoryAdapter).
struct RegisteredAdapter {
    registry: Arc<CdnAdapterRegistry>,
    kind: CdnKind,
}

impl RegisteredAdapter {
    fn new(registry: Arc<CdnAdapterRegistry>, kind: CdnKind) -> Self {
        Self { registry, kind }
    }
}

#[async_trait::async_trait]
impl CdnAdapter for RegisteredAdapter {
    type Error = SiteCacheCdnError;

    async fn list_zones(&self) -> Result<Vec<CdnZone>, Self::Error> {
        self.registry.get(self.kind)?.list_zones().await
    }

    async fn purge(&self, zone: &CdnZone, paths: &[String]) -> Result<PurgeReceipt, Self::Error> {
        self.registry.get(self.kind)?.purge(zone, paths).await
    }

    async fn set_cache_level(&self, zone: &CdnZone, level: CacheLevel) -> Result<(), Self::Error> {
        self.registry
            .get(self.kind)?
            .set_cache_level(zone, level)
            .await
    }

    async fn get_headers(
        &self,
        zone: &CdnZone,
    ) -> Result<openpanel_domain::HeaderSummary, Self::Error> {
        self.registry.get(self.kind)?.get_headers(zone).await
    }
}

/// Map a nginx-apply error into the bounded-context error type.
fn map_nginx_error(error: NginxApplyError) -> SiteCacheCdnError {
    match error {
        NginxApplyError::Io(msg) => SiteCacheCdnError::Adapter(format!("nginx io: {msg}")),
        NginxApplyError::TestFailed(msg) => SiteCacheCdnError::Adapter(msg),
        NginxApplyError::ReloadFailed(msg) => SiteCacheCdnError::Adapter(format!("reload: {msg}")),
    }
}

/// Build a credentialed adapter for a kind from its decrypted config.
fn build_adapter(
    kind: CdnKind,
    config: &CdnProviderConfig,
) -> Box<dyn CdnAdapter<Error = SiteCacheCdnError>> {
    match kind {
        CdnKind::Cloudflare => Box::new(CloudflareAdapter::new(
            config.api_token.clone().unwrap_or_default(),
            config.zone_id.clone().unwrap_or_default(),
        )),
        CdnKind::CloudFront => Box::new(CloudFrontAdapter::new(
            config
                .region
                .clone()
                .unwrap_or_else(|| "us-east-1".to_string()),
            config.zone_id.clone().unwrap_or_default(),
            config.api_token.clone().unwrap_or_default(),
            config.secret_key.clone().unwrap_or_default(),
        )),
        CdnKind::GenericHttp => Box::new(GenericHttpAdapter::new(
            config.webhook_url.clone().unwrap_or_default(),
        )),
    }
}

/// Per-path purge collector: calls the adapter once per path so a
/// single upstream failure does not abort the others. Returns the
/// successful paths, the failure count, and redacted reasons.
async fn collect_purge<A>(
    adapter: &A,
    zone: &CdnZone,
    paths: &[String],
) -> (Vec<String>, usize, Vec<String>)
where
    A: CdnAdapter<Error = SiteCacheCdnError> + ?Sized,
{
    let mut successful = Vec::new();
    let mut failed = 0usize;
    let mut redacted = Vec::new();
    for path in paths {
        match adapter.purge(zone, std::slice::from_ref(path)).await {
            Ok(_) => successful.push(path.clone()),
            Err(e) => {
                failed += 1;
                redacted.push(format!("path={} reason={}", path, redact_reason(&e)));
            }
        }
    }
    (successful, failed, redacted)
}

/// Reduce an adapter error to a secret-free reason token.
fn redact_reason(error: &SiteCacheCdnError) -> &'static str {
    match error {
        SiteCacheCdnError::CryptoFailure(_) => "crypto_error",
        SiteCacheCdnError::Adapter(_) => "adapter_error",
        SiteCacheCdnError::AdapterNotAvailable(_) => "adapter_unavailable",
        SiteCacheCdnError::IntegrationNotFound => "integration_not_found",
        _ => "purge_error",
    }
}

/// Maps a `CdnKind` to its live `CdnAdapter`. Providers register
/// themselves at construction; `get` refuses unknown kinds.
#[derive(Default)]
pub struct CdnAdapterRegistry {
    adapters: std::collections::HashMap<CdnKind, Box<dyn CdnAdapter<Error = SiteCacheCdnError>>>,
}

impl CdnAdapterRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an adapter for a kind.
    pub fn register<A>(&mut self, kind: CdnKind, adapter: A)
    where
        A: CdnAdapter<Error = SiteCacheCdnError>,
    {
        self.adapters.insert(kind, Box::new(adapter));
    }

    /// Resolve an adapter by kind.
    pub fn get(
        &self,
        kind: CdnKind,
    ) -> Result<&dyn CdnAdapter<Error = SiteCacheCdnError>, SiteCacheCdnError> {
        self.adapters.get(&kind).map(|a| a.as_ref()).ok_or_else(|| {
            SiteCacheCdnError::Adapter(format!("no adapter registered for `{}`", kind.as_str()))
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::{super::SqliteSiteCacheCdnRepository, *};

    struct MemoryAdapter {
        zones: Vec<CdnZone>,
        /// Paths that should fail when purged.
        fail: std::collections::HashSet<String>,
    }

    #[async_trait::async_trait]
    impl CdnAdapter for MemoryAdapter {
        type Error = SiteCacheCdnError;

        async fn list_zones(&self) -> Result<Vec<CdnZone>, Self::Error> {
            Ok(self.zones.clone())
        }

        async fn purge(
            &self,
            _zone: &CdnZone,
            paths: &[String],
        ) -> Result<PurgeReceipt, Self::Error> {
            for p in paths {
                if self.fail.contains(p) {
                    return Err(SiteCacheCdnError::Adapter(format!("fail {p}")));
                }
            }
            Ok(PurgeReceipt::new(paths.to_vec(), chrono::Utc::now()))
        }

        async fn set_cache_level(
            &self,
            _zone: &CdnZone,
            _level: CacheLevel,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_headers(
            &self,
            _zone: &CdnZone,
        ) -> Result<openpanel_domain::HeaderSummary, Self::Error> {
            Ok(openpanel_domain::HeaderSummary::new(None, None, None))
        }
    }

    async fn repo() -> Arc<dyn SiteCacheCdnRepository> {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(crate::migrations::SITE_CACHE_CDN_V001)
            .execute(&pool)
            .await
            .unwrap();
        Arc::new(SqliteSiteCacheCdnRepository::new(pool))
    }

    fn service(repo: Arc<dyn SiteCacheCdnRepository>) -> SiteCacheService {
        let mut registry = CdnAdapterRegistry::new();
        registry.register(
            CdnKind::GenericHttp,
            MemoryAdapter {
                zones: vec![CdnZone::new("z1", "example.com")],
                fail: std::collections::HashSet::new(),
            },
        );
        SiteCacheService::new(
            repo,
            Arc::new(openpanel_test_support::MockAudit::stub()),
            Arc::new(registry),
        )
    }

    #[tokio::test]
    async fn policy_defaults_then_persists() {
        let svc = service(repo().await);
        let site_id = Uuid::new_v4();
        let policy = svc.cache_policy(site_id).await.unwrap();
        assert_eq!(policy.ttl_seconds(), 60);
        let updated = SiteCachePolicy::with_ttl(site_id, 300)
            .unwrap()
            .with_bypass_paths(vec!["/wp-admin/*".to_string()])
            .unwrap();
        svc.set_cache_policy("admin", &updated).await.unwrap();
        let loaded = svc.cache_policy(site_id).await.unwrap();
        assert_eq!(loaded.ttl_seconds(), 300);
        assert!(loaded.bypasses("/wp-admin/index.php"));
    }

    #[tokio::test]
    async fn integration_lifecycle() {
        let svc = service(repo().await);
        let integration = svc
            .create_integration_for_provider(
                "admin",
                "prod",
                CdnKind::GenericHttp,
                None,
                None,
                Some("https://hook.example.com".to_string()),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(integration.kind(), CdnKind::GenericHttp);
        assert!(svc.list_integrations().await.unwrap().len() == 1);
        svc.delete_integration("admin", integration.id())
            .await
            .unwrap();
        assert!(svc.list_integrations().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_adapter_kind_rejected_at_create() {
        let svc = service(repo().await);
        // Only `generic_http` is registered in the test registry.
        let err = svc
            .create_integration_for_provider(
                "admin",
                "cf",
                CdnKind::Cloudflare,
                Some("tok".into()),
                Some("z1".into()),
                None,
                None,
                None,
            )
            .await
            .expect_err("cloudflare not registered");
        assert!(matches!(err, SiteCacheCdnError::AdapterNotAvailable(_)));
    }

    #[tokio::test]
    async fn purge_records_receipt() {
        let svc = service(repo().await);
        let integration = svc
            .create_integration_for_provider(
                "admin",
                "prod",
                CdnKind::GenericHttp,
                None,
                None,
                Some("https://hook.example.com".to_string()),
                None,
                None,
            )
            .await
            .unwrap();
        let adapter = svc.registry();
        let adapter = adapter.get(CdnKind::GenericHttp).unwrap();
        let zone = CdnZone::new("z1", "example.com");
        let request = PurgeRequest::new(integration.id(), vec!["/article/x".to_string()]).unwrap();
        let receipt = svc.purge("admin", adapter, &zone, &request).await.unwrap();
        assert_eq!(receipt.purged(), &["/article/x"]);
        let recent = svc.recent_purges(integration.id()).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].purged(), &["/article/x"]);
    }

    #[tokio::test]
    async fn purge_via_integration_partial_records_redacted() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(crate::migrations::SITE_CACHE_CDN_V001)
            .execute(&pool)
            .await
            .unwrap();
        let repo = Arc::new(SqliteSiteCacheCdnRepository::new(pool));
        let mut registry = CdnAdapterRegistry::new();
        let mut fail = std::collections::HashSet::new();
        fail.insert("/y".to_string());
        registry.register(
            CdnKind::GenericHttp,
            MemoryAdapter {
                zones: vec![CdnZone::new("z1", "example.com")],
                fail,
            },
        );
        let svc = SiteCacheService::new(
            repo,
            Arc::new(openpanel_test_support::MockAudit::stub()),
            Arc::new(registry),
        );
        let integration = svc
            .create_integration_for_provider(
                "admin",
                "prod",
                CdnKind::GenericHttp,
                None,
                None,
                Some("https://hook.example.com".to_string()),
                None,
                None,
            )
            .await
            .unwrap();
        // `purge_via_integration` honours the registry's adapter for
        // an integration's kind, so a custom MemoryAdapter that fails
        // `/y` is reachable here.
        let summary = svc
            .purge_via_integration(
                "admin",
                integration.id(),
                vec!["/a".to_string(), "/y".to_string()],
            )
            .await
            .unwrap();
        assert_eq!(summary.successful, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.redacted.len(), 1);
        assert!(summary.redacted[0].starts_with("path=/y reason="));
    }
}
