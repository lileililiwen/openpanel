//! DNS module composition and deterministic provider adapter.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::dns::{DnsName, ProviderCapabilities, RemoteVersion};
use uuid::Uuid;

use super::{
    CloudflareDohResolver, CloudflareProvider, CredentialCipher, DnsProvider, DnsResolver,
    DnsService, DnsServiceError, MemoryDnsRepository, ProviderCredential, ProviderZone,
    RemoteRecord, SqliteDnsRepository,
};

/// Stable DNS module name.
pub const MODULE_NAME: &str = "dns";

/// Deterministic provider used in integration tests and explicitly selected local workflows.
pub struct FakeDnsProvider {
    zone: ProviderZone,
    records: Mutex<HashMap<String, RemoteRecord>>,
}
impl FakeDnsProvider {
    /// Construct the deterministic `example.test` provider.
    pub fn new() -> Result<Self, DnsServiceError> {
        Ok(Self {
            zone: ProviderZone::new(
                Uuid::new_v4(),
                "example-test",
                DnsName::new("example.test").map_err(|_| DnsServiceError::Invalid)?,
                RemoteVersion::new("v1").map_err(|_| DnsServiceError::Invalid)?,
            ),
            records: Mutex::new(HashMap::new()),
        })
    }
}
#[async_trait]
impl DnsProvider for FakeDnsProvider {
    async fn test(
        &self,
        credential: &ProviderCredential,
    ) -> Result<ProviderCapabilities, DnsServiceError> {
        if credential.expose().is_empty() {
            Err(DnsServiceError::provider("permission"))
        } else {
            Ok(ProviderCapabilities::all(60, 86_400))
        }
    }

    async fn zones(
        &self,
        _credential: &ProviderCredential,
    ) -> Result<Vec<ProviderZone>, DnsServiceError> {
        Ok(vec![self.zone.clone()])
    }

    async fn records(
        &self,
        _credential: &ProviderCredential,
        _zone_id: &str,
    ) -> Result<Vec<RemoteRecord>, DnsServiceError> {
        Ok(self
            .records
            .lock()
            .map_err(|_| DnsServiceError::Provider("provider request failed".into()))?
            .values()
            .cloned()
            .collect())
    }

    async fn create_record(
        &self,
        _credential: &ProviderCredential,
        zone_id: &str,
        record: &RemoteRecord,
        expected: &RemoteVersion,
    ) -> Result<RemoteRecord, DnsServiceError> {
        if zone_id != self.zone.remote_id || expected != &self.zone.remote_version {
            return Err(DnsServiceError::Conflict);
        }
        let mut created = record.clone();
        created.remote_id = Uuid::new_v4().to_string();
        self.records
            .lock()
            .map_err(|_| DnsServiceError::Provider("provider request failed".into()))?
            .insert(created.remote_id.clone(), created.clone());
        Ok(created)
    }

    async fn update_record(
        &self,
        _credential: &ProviderCredential,
        zone_id: &str,
        record: &RemoteRecord,
        expected: &RemoteVersion,
    ) -> Result<RemoteRecord, DnsServiceError> {
        if zone_id != self.zone.remote_id || expected != &self.zone.remote_version {
            return Err(DnsServiceError::Conflict);
        }
        self.records
            .lock()
            .map_err(|_| DnsServiceError::Provider("provider request failed".into()))?
            .insert(record.remote_id.clone(), record.clone());
        Ok(record.clone())
    }

    async fn delete_record(
        &self,
        _credential: &ProviderCredential,
        zone_id: &str,
        record_id: &str,
        expected: &RemoteVersion,
    ) -> Result<RemoteVersion, DnsServiceError> {
        if zone_id != self.zone.remote_id || expected != &self.zone.remote_version {
            return Err(DnsServiceError::Conflict);
        }
        self.records
            .lock()
            .map_err(|_| DnsServiceError::Provider("provider request failed".into()))?
            .remove(record_id);
        Ok(self.zone.remote_version.clone())
    }
}

/// DNS service and schema module.
pub struct DnsModule {
    service: Arc<DnsService>,
    migrations: Vec<Migration>,
}
impl DnsModule {
    /// Compose the SQLite-backed service.
    pub async fn new(ctx: &AppContext) -> Result<Self, DnsServiceError> {
        let fake = std::env::var("OPENPANEL__DNS__PROVIDER").as_deref() == Ok("fake");
        let provider: Arc<dyn DnsProvider> = if fake {
            Arc::new(FakeDnsProvider::new()?)
        } else {
            Arc::new(CloudflareProvider::default())
        };
        let resolver: Arc<dyn DnsResolver> = if fake {
            Arc::new(super::resolver::SynchronizedResolver)
        } else {
            Arc::new(CloudflareDohResolver::default())
        };
        Self::compose_with_provider(
            ctx,
            Arc::new(SqliteDnsRepository::new(ctx.db.pool().await)),
            provider,
            resolver,
        )
        .await
    }

    /// Compose deterministic in-memory DNS for integration tests.
    pub async fn memory(ctx: &AppContext) -> Result<Self, DnsServiceError> {
        Self::compose(ctx, Arc::new(MemoryDnsRepository::default())).await
    }

    async fn compose(
        ctx: &AppContext,
        repo: Arc<dyn super::DnsRepository>,
    ) -> Result<Self, DnsServiceError> {
        Self::compose_with_provider(
            ctx,
            repo,
            Arc::new(FakeDnsProvider::new()?),
            Arc::new(super::resolver::SynchronizedResolver),
        )
        .await
    }

    async fn compose_with_provider(
        ctx: &AppContext,
        repo: Arc<dyn super::DnsRepository>,
        provider: Arc<dyn DnsProvider>,
        resolver: Arc<dyn DnsResolver>,
    ) -> Result<Self, DnsServiceError> {
        let key = std::env::var("OPENPANEL__DATABASE__MASTER_KEY")
            .map(|value| {
                use sha2::{Digest, Sha256};
                Sha256::digest(value.as_bytes()).into()
            })
            .unwrap_or([42_u8; 32]);
        Ok(Self {
            service: Arc::new(
                DnsService::new(
                    repo,
                    provider,
                    CredentialCipher::new(&key)?,
                    ctx.audit.clone(),
                )
                .with_resolver(resolver),
            ),
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "DNS providers, zones, and records".into(),
                sql: crate::migrations::DNS_V001.into(),
            }],
        })
    }

    /// Shared DNS application service.
    pub fn service(&self) -> Arc<DnsService> {
        self.service.clone()
    }
}
impl Module for DnsModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
