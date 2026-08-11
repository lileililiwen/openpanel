//! Catalog source port: where the Software Center fetches its data from.
//!
//! The panel aggregates catalogs; the source is the *only* place recipes
//! come from. The kernel still does the signature/schema/expiry work; this
//! module is the network boundary that produces an unsigned, validated
//! `CatalogManifest` ready to be activated.

use std::{collections::BTreeSet, time::Duration};

use async_trait::async_trait;
use openpanel_domain::software_center::CatalogManifest;
use serde::{Deserialize, Serialize};

use super::SoftwareCenterError;

/// Hard upper bound on the manifest body size. Matches the kernel's
/// envelope cap; the source layer enforces it before the signature check
/// sees the bytes.
pub const MAX_MANIFEST_BYTES: usize = 1_048_576;

/// Hard upper bound on the number of entries in a single manifest.
pub const MAX_ENTRIES: usize = 256;

/// Bounded fetch + verification result.
#[derive(Debug, Clone)]
pub struct FetchedManifest {
    /// Validated manifest ready to be activated.
    pub manifest: CatalogManifest,
    /// Source URL the manifest was retrieved from.
    pub source_url: String,
    /// Origin host extracted from `source_url` for diagnostics.
    pub origin: String,
}

/// Catalog source port. Implementations return either a validated manifest
/// or a typed error. The host (OpenPanel) never executes content from the
/// manifest; it only deserializes the data-only JSON.
#[async_trait]
pub trait CatalogSource: Send + Sync {
    /// Fetch and validate one manifest from the configured source.
    async fn fetch(&self, now_unix: u64) -> Result<FetchedManifest, SoftwareCenterError>;

    /// Stable identifier for the source, used in logs and diagnostics.
    fn source_id(&self) -> &'static str;

    /// Display URL for the source, used in the diagnostics strip.
    fn source_url(&self) -> &str;
}

/// Configuration for the HTTP source.
#[derive(Debug, Clone)]
pub struct HttpSourceConfig {
    /// HTTPS manifest URL.
    pub url: String,
    /// Allowlist of expected hostnames. The fetch is rejected unless the
    /// URL's host is on this list.
    pub allowed_origins: BTreeSet<String>,
    /// Connect timeout.
    pub connect_timeout: Duration,
    /// Read timeout.
    pub read_timeout: Duration,
}

impl HttpSourceConfig {
    /// Construct a config with safe defaults.
    pub fn new(url: impl Into<String>, allowed_origins: BTreeSet<String>) -> Self {
        Self {
            url: url.into(),
            allowed_origins,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(60),
        }
    }
}

/// Manifest wrapper used by the HTTP source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpManifestEnvelope {
    /// Schema version (always `1` in v1).
    pub schema: u32,
    /// Manifest source URL.
    pub source_url: String,
    /// Issuance timestamp (RFC 3339).
    pub issued_at: String,
    /// Entries.
    pub entries: Vec<openpanel_domain::software_center::CatalogEntryRecipe>,
}

/// HTTP catalog source with a TLS-only, redirect-free, origin-allowlisted
/// reqwest client. Returns the manifest wrapped in a `FetchedManifest`.
pub struct HttpCatalogSource {
    config: HttpSourceConfig,
    client: reqwest::Client,
}

impl HttpCatalogSource {
    /// Build a new HTTP source. Returns an error when the client cannot
    /// be constructed.
    pub fn new(config: HttpSourceConfig) -> Result<Self, SoftwareCenterError> {
        let client = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .user_agent(concat!("openpanel/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
        Ok(Self { config, client })
    }
}

#[async_trait]
impl CatalogSource for HttpCatalogSource {
    async fn fetch(&self, _now_unix: u64) -> Result<FetchedManifest, SoftwareCenterError> {
        let parsed =
            reqwest::Url::parse(&self.config.url).map_err(|_| SoftwareCenterError::Invalid)?;
        if parsed.scheme() != "https" || !parsed.username().is_empty() {
            return Err(SoftwareCenterError::Invalid);
        }
        let host = parsed.host_str().ok_or(SoftwareCenterError::Invalid)?;
        if !self.config.allowed_origins.contains(host) {
            return Err(SoftwareCenterError::Invalid);
        }
        let response = self
            .client
            .get(parsed.clone())
            .send()
            .await
            .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
        if !response.status().is_success() {
            return Err(SoftwareCenterError::Package("operation failed".into()));
        }
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > MAX_MANIFEST_BYTES as u64) {
            return Err(SoftwareCenterError::Invalid);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(SoftwareCenterError::Invalid);
        }
        let manifest: HttpManifestEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| SoftwareCenterError::Invalid)?;
        if manifest.schema != 1 {
            return Err(SoftwareCenterError::Invalid);
        }
        let entries = manifest.entries;
        if entries.is_empty() || entries.len() > MAX_ENTRIES {
            return Err(SoftwareCenterError::Invalid);
        }
        let domain_manifest = CatalogManifest {
            schema: 1,
            issued_at: manifest.issued_at,
            source_url: parsed.to_string(),
            entries,
        };
        domain_manifest
            .validate()
            .map_err(|_| SoftwareCenterError::Invalid)?;
        Ok(FetchedManifest {
            manifest: domain_manifest,
            source_url: parsed.to_string(),
            origin: host.to_owned(),
        })
    }

    fn source_id(&self) -> &'static str {
        "http"
    }

    fn source_url(&self) -> &str {
        &self.config.url
    }
}

/// Embedded recovery seed. Implements `CatalogSource` by returning a static
/// manifest built from `seed::embedded_manifest()`. The source is the
/// last-resort fallback used on first boot when no remote manifest has
/// ever been activated and the network is unavailable.
pub struct EmbeddedCatalogSource {
    source_url: String,
}

impl EmbeddedCatalogSource {
    /// Build an embedded source. The source URL is the canonical label
    /// shown in the diagnostics strip when the seed is in use.
    pub fn new(source_url: impl Into<String>) -> Self {
        Self {
            source_url: source_url.into(),
        }
    }
}

#[async_trait]
impl CatalogSource for EmbeddedCatalogSource {
    async fn fetch(&self, _now_unix: u64) -> Result<FetchedManifest, SoftwareCenterError> {
        let manifest = super::seed::embedded_manifest()?;
        Ok(FetchedManifest {
            source_url: self.source_url.clone(),
            origin: "embedded".to_owned(),
            manifest,
        })
    }

    fn source_id(&self) -> &'static str {
        "embedded"
    }

    fn source_url(&self) -> &str {
        &self.source_url
    }
}

/// In-memory source for tests and development. Holds a `CatalogManifest`
/// and returns it on every fetch.
pub struct StaticCatalogSource {
    source_url: String,
    manifest: CatalogManifest,
}

impl StaticCatalogSource {
    /// Build a static source from a pre-built manifest.
    pub fn new(source_url: impl Into<String>, manifest: CatalogManifest) -> Self {
        Self {
            source_url: source_url.into(),
            manifest,
        }
    }
}

#[async_trait]
impl CatalogSource for StaticCatalogSource {
    async fn fetch(&self, _now_unix: u64) -> Result<FetchedManifest, SoftwareCenterError> {
        Ok(FetchedManifest {
            source_url: self.source_url.clone(),
            origin: "static".to_owned(),
            manifest: self.manifest.clone(),
        })
    }

    fn source_id(&self) -> &'static str {
        "static"
    }

    fn source_url(&self) -> &str {
        &self.source_url
    }
}
