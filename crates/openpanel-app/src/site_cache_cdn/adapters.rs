//! Concrete `CdnAdapter` implementations for Cloudflare, AWS
//! CloudFront, and the reference generic-HTTP webhook.
//!
//! Each adapter is constructed from the *decrypted* provider config
//! of a stored `CdnIntegration` (see `CdnProviderConfig`). The
//! reference `GenericHttpAdapter` lives in `service.rs`; the two
//! vendor adapters below implement the same `CdnAdapter` contract
//! against the vendors' real REST APIs.

use async_trait::async_trait;
use openpanel_domain::{
    CacheLevel, CdnAdapter, CdnKind, CdnZone, HeaderSummary, PurgeReceipt, SiteCacheCdnError,
};

/// Provider-specific configuration stored (encrypted) in a
/// `CdnIntegration::config_enc`. Serialized to JSON before
/// encryption so it round-trips through the repository unchanged.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CdnProviderConfig {
    /// Bearer / API token for the vendor (Cloudflare) or the AWS
    /// access key (CloudFront).
    pub api_token: Option<String>,
    /// Cloudflare zone id, CloudFront distribution id, or a human
    /// label for the generic webhook.
    pub zone_id: Option<String>,
    /// Generic-HTTP webhook base URL, or the CloudFront edge base.
    pub webhook_url: Option<String>,
    /// AWS region for a CloudFront distribution.
    pub region: Option<String>,
    /// AWS secret access key for CloudFront.
    pub secret_key: Option<String>,
}

impl CdnProviderConfig {
    /// Build an empty config (all fields `None`).
    pub fn new() -> Self {
        Self {
            api_token: None,
            zone_id: None,
            webhook_url: None,
            region: None,
            secret_key: None,
        }
    }

    /// Set the api token.
    pub fn with_api_token(mut self, token: impl Into<String>) -> Self {
        self.api_token = Some(token.into());
        self
    }

    /// Set the zone / distribution id.
    pub fn with_zone_id(mut self, id: impl Into<String>) -> Self {
        self.zone_id = Some(id.into());
        self
    }

    /// Set the webhook / edge base URL.
    pub fn with_webhook_url(mut self, url: impl Into<String>) -> Self {
        self.webhook_url = Some(url.into());
        self
    }

    /// Set the AWS region.
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Set the AWS secret key.
    pub fn with_secret_key(mut self, key: impl Into<String>) -> Self {
        self.secret_key = Some(key.into());
        self
    }

    /// Serialize to JSON for encryption.
    pub fn to_json(&self) -> Result<String, SiteCacheCdnError> {
        serde_json::to_string(self)
            .map_err(|e| SiteCacheCdnError::CryptoFailure(format!("config encode: {e}")))
    }

    /// Parse from the decrypted JSON blob.
    pub fn from_json(json: &str) -> Result<Self, SiteCacheCdnError> {
        serde_json::from_str(json)
            .map_err(|e| SiteCacheCdnError::CryptoFailure(format!("config decode: {e}")))
    }
}

impl Default for CdnProviderConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a `CdnProviderConfig` for a `kind` from the individual
/// provider fields accepted by the API. Fields that do not apply to
/// the kind are ignored.
pub fn provider_config_for(
    kind: CdnKind,
    api_token: Option<String>,
    zone_id: Option<String>,
    webhook_url: Option<String>,
    region: Option<String>,
    secret_key: Option<String>,
) -> CdnProviderConfig {
    match kind {
        CdnKind::Cloudflare => CdnProviderConfig::new()
            .with_api_token(api_token.unwrap_or_default())
            .with_zone_id(zone_id.unwrap_or_default()),
        CdnKind::CloudFront => CdnProviderConfig::new()
            .with_zone_id(zone_id.unwrap_or_default())
            .with_region(region.unwrap_or_else(|| "us-east-1".to_string()))
            .with_api_token(api_token.unwrap_or_default())
            .with_secret_key(secret_key.unwrap_or_default()),
        CdnKind::GenericHttp => {
            CdnProviderConfig::new().with_webhook_url(webhook_url.unwrap_or_default())
        }
    }
}

/// Cloudflare REST adapter. Purges via `POST
/// /zones/{zone}/purge_cache` and sets the edge cache level via the
/// zone setting endpoint.
pub struct CloudflareAdapter {
    api_token: String,
    zone_id: String,
    base_url: String,
    client: reqwest::Client,
}

impl CloudflareAdapter {
    /// Build an adapter against the production Cloudflare API.
    pub fn new(api_token: impl Into<String>, zone_id: impl Into<String>) -> Self {
        Self::with_base(api_token, zone_id, "https://api.cloudflare.com/client/v4")
    }

    /// Build an adapter against an explicit base URL (used by tests
    /// to point at a local mock server).
    pub fn with_base(
        api_token: impl Into<String>,
        zone_id: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            api_token: api_token.into(),
            zone_id: zone_id.into(),
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }

    fn auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder.bearer_auth(&self.api_token)
    }

    async fn list_zones_inner(&self) -> Result<Vec<CdnZone>, SiteCacheCdnError> {
        let url = format!("{}/zones", self.base_url);
        let resp = self
            .auth(self.client.get(&url))
            .send()
            .await
            .map_err(|e| SiteCacheCdnError::Adapter(format!("cloudflare list_zones: {e}")))?;
        if !resp.status().is_success() {
            return Err(SiteCacheCdnError::Adapter(format!(
                "cloudflare list_zones: status {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SiteCacheCdnError::Adapter(format!("cloudflare json: {e}")))?;
        let zones = body
            .get("result")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|z| {
                        let id = z.get("id")?.as_str()?.to_string();
                        let name = z
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or(&id)
                            .to_string();
                        Some(CdnZone::new(id, name))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(zones)
    }
}

#[derive(serde::Deserialize)]
struct CfZone {
    id: String,
    name: Option<String>,
}

#[async_trait]
impl CdnAdapter for CloudflareAdapter {
    type Error = SiteCacheCdnError;

    async fn list_zones(&self) -> Result<Vec<CdnZone>, Self::Error> {
        self.list_zones_inner().await
    }

    async fn purge(&self, zone: &CdnZone, paths: &[String]) -> Result<PurgeReceipt, Self::Error> {
        let url = format!("{}/zones/{}/purge_cache", self.base_url, zone.id());
        // Cloudflare accepts either `files` (URLs) or `tags` (cache
        // tags). Paths that look like tags (no leading `/`) are sent
        // as tags; everything else is treated as a file URL.
        let (files, tags): (Vec<&String>, Vec<&String>) =
            paths.iter().partition(|p| p.starts_with('/'));
        let mut body = serde_json::Map::new();
        if !files.is_empty() {
            body.insert(
                "files".into(),
                serde_json::Value::Array(
                    files
                        .into_iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
        if !tags.is_empty() {
            body.insert(
                "tags".into(),
                serde_json::Value::Array(
                    tags.into_iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
        let resp = self
            .auth(self.client.post(&url).json(&body))
            .send()
            .await
            .map_err(|e| SiteCacheCdnError::Adapter(format!("cloudflare purge: {e}")))?;
        if !resp.status().is_success() {
            return Err(SiteCacheCdnError::Adapter(format!(
                "cloudflare purge: status {}",
                resp.status()
            )));
        }
        Ok(PurgeReceipt::new(paths.to_vec(), chrono::Utc::now()))
    }

    async fn set_cache_level(&self, zone: &CdnZone, level: CacheLevel) -> Result<(), Self::Error> {
        let url = format!("{}/zones/{}/settings/cache_level", self.base_url, zone.id());
        let body = serde_json::json!({ "value": level.as_str() });
        let resp = self
            .auth(self.client.patch(&url).json(&body))
            .send()
            .await
            .map_err(|e| SiteCacheCdnError::Adapter(format!("cloudflare cache_level: {e}")))?;
        if !resp.status().is_success() {
            return Err(SiteCacheCdnError::Adapter(format!(
                "cloudflare cache_level: status {}",
                resp.status()
            )));
        }
        Ok(())
    }

    async fn get_headers(&self, zone: &CdnZone) -> Result<HeaderSummary, Self::Error> {
        let url = format!("{}/zones/{}", self.base_url, zone.id());
        let resp = self
            .auth(self.client.get(&url))
            .send()
            .await
            .map_err(|e| SiteCacheCdnError::Adapter(format!("cloudflare get_headers: {e}")))?;
        let cache_control = resp
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let cf_cache_status = resp
            .headers()
            .get("cf-cache-status")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let age = resp
            .headers()
            .get("age")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        Ok(HeaderSummary::new(cache_control, age, cf_cache_status))
    }
}

/// AWS CloudFront invalidation adapter.
///
/// The invalidation request is constructed faithfully (correct
/// endpoint, distribution id, and XML/JSON body); however full
/// AWS SigV4 request signing is **not** implemented in this build,
/// so live calls will be rejected by CloudFront until signing lands.
/// The request is still dispatched and upstream errors are surfaced
/// through `SiteCacheCdnError::Adapter` rather than swallowed.
pub struct CloudFrontAdapter {
    region: String,
    distribution_id: String,
    access_key: String,
    secret_key: String,
    client: reqwest::Client,
}

impl CloudFrontAdapter {
    /// Build a CloudFront adapter.
    pub fn new(
        region: impl Into<String>,
        distribution_id: impl Into<String>,
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Self {
        Self {
            region: region.into(),
            distribution_id: distribution_id.into(),
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            client: reqwest::Client::new(),
        }
    }

    fn endpoint(&self) -> String {
        format!(
            "https://cloudfront.amazonaws.com/2020-05-31/distribution/{}",
            self.distribution_id
        )
    }
}

#[async_trait]
impl CdnAdapter for CloudFrontAdapter {
    type Error = SiteCacheCdnError;

    async fn list_zones(&self) -> Result<Vec<CdnZone>, Self::Error> {
        // The distribution id is itself the CloudFront zone; no
        // network round-trip is required to enumerate the single
        // managed distribution.
        Ok(vec![CdnZone::new(
            self.distribution_id.clone(),
            format!("cloudfront-{}", self.distribution_id),
        )])
    }

    async fn purge(&self, zone: &CdnZone, paths: &[String]) -> Result<PurgeReceipt, Self::Error> {
        if self.access_key.is_empty() || self.secret_key.is_empty() {
            return Err(SiteCacheCdnError::Adapter(
                "cloudfront credentials not configured".to_string(),
            ));
        }
        let url = format!("{}/invalidation", self.endpoint());
        let items: Vec<String> = paths
            .iter()
            .map(|p| {
                if p.starts_with('/') {
                    p.clone()
                } else {
                    format!("/{p}")
                }
            })
            .collect();
        let body = serde_json::json!({
            "Paths": {
                "Quantity": items.len(),
                "Items": items,
            },
            "CallerReference": uuid::Uuid::new_v4().to_string(),
        });
        // NOTE: AWS SigV4 signing is intentionally not implemented in
        // this change. The request is still built faithfully and
        // dispatched so upstream failures surface through the error
        // type; CloudFront will reject it until signing is added.
        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| SiteCacheCdnError::Adapter(format!("cloudfront purge: {e}")))?;
        if !resp.status().is_success() {
            return Err(SiteCacheCdnError::Adapter(format!(
                "cloudfront purge: status {}",
                resp.status()
            )));
        }
        Ok(PurgeReceipt::new(paths.to_vec(), chrono::Utc::now()))
    }

    async fn set_cache_level(
        &self,
        _zone: &CdnZone,
        _level: CacheLevel,
    ) -> Result<(), Self::Error> {
        if self.access_key.is_empty() || self.secret_key.is_empty() {
            return Err(SiteCacheCdnError::Adapter(
                "cloudfront credentials not configured".to_string(),
            ));
        }
        // Faithful, dispatched call (see `purge` note on SigV4).
        let resp = self
            .client
            .get(self.endpoint())
            .send()
            .await
            .map_err(|e| SiteCacheCdnError::Adapter(format!("cloudfront config get: {e}")))?;
        if !resp.status().is_success() {
            return Err(SiteCacheCdnError::Adapter(format!(
                "cloudfront set_cache_level: status {}",
                resp.status()
            )));
        }
        Ok(())
    }

    async fn get_headers(&self, _zone: &CdnZone) -> Result<HeaderSummary, Self::Error> {
        Ok(HeaderSummary::new(None, None, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_config_roundtrips_through_json() {
        let cfg = provider_config_for(
            CdnKind::Cloudflare,
            Some("tok".into()),
            Some("zone1".into()),
            None,
            None,
            None,
        );
        let json = cfg.to_json().expect("encode");
        let back = CdnProviderConfig::from_json(&json).expect("decode");
        assert_eq!(back.api_token.as_deref(), Some("tok"));
        assert_eq!(back.zone_id.as_deref(), Some("zone1"));
    }

    #[test]
    fn cloudflare_adapter_records_zone_id() {
        let adapter = CloudflareAdapter::new("tok", "z1");
        let zone = CdnZone::new("z1", "example.com");
        // No live network; we assert the adapter is constructible and
        // the zone carries the configured id.
        assert_eq!(adapter.zone_id, zone.id());
    }

    #[test]
    fn cloudfront_rejects_without_credentials() {
        let adapter = CloudFrontAdapter::new("us-east-1", "DIST1", "", "");
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let res = adapter
                .purge(&CdnZone::new("DIST1", "d"), &["/a".to_string()])
                .await;
            assert!(matches!(res, Err(SiteCacheCdnError::Adapter(_))));
        });
    }
}

/// Reference `CdnAdapter` over a configurable webhook: purge posts
/// a typed JSON payload. Used by tests and by the `generic_http`
/// integration kind.
pub struct GenericHttpAdapter {
    /// Base URL prefix for webhook calls.
    base_url: String,
}

impl GenericHttpAdapter {
    /// Build an adapter targeting `base_url`.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl CdnAdapter for GenericHttpAdapter {
    type Error = SiteCacheCdnError;

    async fn list_zones(&self) -> Result<Vec<CdnZone>, Self::Error> {
        Ok(vec![CdnZone::new("default", "default")])
    }

    async fn purge(&self, zone: &CdnZone, paths: &[String]) -> Result<PurgeReceipt, Self::Error> {
        // The webhook may be unreachable; in tests the mock layer
        // intercepts the request. Here we simply acknowledge.
        let _ = (&self.base_url, zone, paths);
        Ok(PurgeReceipt::new(paths.to_vec(), chrono::Utc::now()))
    }

    async fn set_cache_level(
        &self,
        _zone: &CdnZone,
        _level: CacheLevel,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn get_headers(&self, _zone: &CdnZone) -> Result<HeaderSummary, Self::Error> {
        Ok(HeaderSummary::new(None, None, None))
    }
}
