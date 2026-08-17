//! HTTP client for the marketplace catalog.

use async_trait::async_trait;
use openpanel_domain::{SignedCatalogEnvelope, plugin_marketplace::error::PluginMarketplaceError};

/// Transport used by the marketplace service to fetch a signed
/// envelope. Production code wires [`HttpMarketplaceClient`]; tests
/// wire [`MockMarketplaceClient`].
#[async_trait]
pub trait MarketplaceClient: Send + Sync {
    /// Fetch the latest signed envelope from the configured
    /// marketplace endpoint.
    async fn fetch_envelope(&self) -> Result<SignedCatalogEnvelope, PluginMarketplaceError>;
}

/// Production HTTP client that fetches the signed envelope over
/// HTTPS. Connection pooling and ETag caching are intentionally
/// out of scope for this change; the follow-on `add-offsite-backup`
/// and `add-cache-and-cdn` changes carry the HTTP stack.
#[derive(Clone)]
pub struct HttpMarketplaceClient {
    endpoint: String,
    client: reqwest::Client,
}

impl HttpMarketplaceClient {
    /// Construct a new client pointing at `endpoint`. The endpoint
    /// MUST be HTTPS in production (the panel refuses plain HTTP).
    pub fn new(endpoint: impl Into<String>) -> Result<Self, PluginMarketplaceError> {
        let endpoint = endpoint.into();
        if !endpoint.starts_with("https://") && !endpoint.starts_with("http://localhost") {
            return Err(PluginMarketplaceError::InvalidCatalog(format!(
                "marketplace endpoint must be https: {endpoint}"
            )));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| PluginMarketplaceError::Persistence(e.to_string()))?;
        Ok(Self { endpoint, client })
    }
}

#[async_trait]
impl MarketplaceClient for HttpMarketplaceClient {
    async fn fetch_envelope(&self) -> Result<SignedCatalogEnvelope, PluginMarketplaceError> {
        let resp = self
            .client
            .get(&self.endpoint)
            .send()
            .await
            .map_err(|e| PluginMarketplaceError::Persistence(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(PluginMarketplaceError::InvalidCatalog(format!(
                "marketplace returned {}",
                resp.status()
            )));
        }
        let envelope: SignedCatalogEnvelope = resp
            .json()
            .await
            .map_err(|e| PluginMarketplaceError::InvalidCatalog(e.to_string()))?;
        Ok(envelope)
    }
}

/// In-memory mock client. Useful for tests that want to drive the
/// service without standing up a HTTP server.
#[derive(Clone, Default)]
pub struct MockMarketplaceClient {
    envelope: std::sync::Arc<std::sync::Mutex<Option<SignedCatalogEnvelope>>>,
    error: std::sync::Arc<std::sync::Mutex<Option<PluginMarketplaceError>>>,
}

impl MockMarketplaceClient {
    /// Construct an empty mock.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the envelope the next call will return.
    pub fn with_envelope(self, envelope: SignedCatalogEnvelope) -> Self {
        {
            #[allow(clippy::unwrap_used)] // mock; mutex is never poisoned
            let mut guard = self.envelope.lock().unwrap();
            *guard = Some(envelope);
        }
        self
    }

    /// Make the next call return `error` instead of an envelope.
    pub fn with_error(self, error: PluginMarketplaceError) -> Self {
        {
            #[allow(clippy::unwrap_used)] // mock; mutex is never poisoned
            let mut guard = self.error.lock().unwrap();
            *guard = Some(error);
        }
        self
    }
}

#[async_trait]
impl MarketplaceClient for MockMarketplaceClient {
    async fn fetch_envelope(&self) -> Result<SignedCatalogEnvelope, PluginMarketplaceError> {
        #[allow(clippy::unwrap_used)] // mock; mutex is never poisoned
        if let Some(err) = self.error.lock().unwrap().take() {
            return Err(err);
        }
        #[allow(clippy::unwrap_used)] // mock; mutex is never poisoned
        self.envelope
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| PluginMarketplaceError::InvalidCatalog("no envelope queued".into()))
    }
}
