//! `SslModule` — composition-root wiring for the ssl bounded context.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{
    super::databases::crypto::KEY_LEN,
    SslPaths,
    acme::{AcmeClient, AcmeEndpoint, RustlsAcmeClient},
    challenge_server::AcmeHttpServer,
    renewal::SslRenewalTask,
    repo::SqliteCertificateRepository,
    service::SslService,
};

/// Stable identifier for the ssl module used in migration bookkeeping
/// and the `/api/v1/ssl` route namespace.
pub const MODULE_NAME: &str = "ssl";

/// Default port for the ACME HTTP-01 challenge server.
pub const CHALLENGE_SERVER_PORT: u16 = 9080;

/// ssl bounded-context module: wires the service, repository,
/// challenge server, and renewal task.
pub struct SslModule {
    service: Arc<SslService>,
    challenge_server: AcmeHttpServer,
    #[allow(dead_code)]
    renewal_task: SslRenewalTask,
    migrations: Vec<Migration>,
}

impl SslModule {
    /// Construct the module using the real `RustlsAcmeClient` (skeleton
    /// — see [`crate::ssl::acme::RustlsAcmeClient`]).
    pub async fn new(
        ctx: &AppContext,
        master_key: [u8; KEY_LEN],
        contact_email: impl Into<String>,
    ) -> Self {
        Self::with_paths(
            ctx,
            SslPaths::default_paths(),
            AcmeEndpoint::default_safe(),
            master_key,
            contact_email,
        )
        .await
    }

    /// Construct with custom filesystem paths and ACME endpoint.
    /// `acme_client` is the ACME adapter (production uses
    /// `RustlsAcmeClient`; tests pass a `MockAcmeClient`).
    pub async fn with_paths(
        ctx: &AppContext,
        paths: SslPaths,
        acme_endpoint: AcmeEndpoint,
        master_key: [u8; KEY_LEN],
        contact_email: impl Into<String>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn openpanel_domain::CertificateRepository> =
            Arc::new(SqliteCertificateRepository::new(pool));
        let challenge_server = AcmeHttpServer::new();
        let contact_email = contact_email.into();
        let acme_client: Arc<dyn AcmeClient> =
            Arc::new(RustlsAcmeClient::new(acme_endpoint, contact_email.clone()));
        let service = Arc::new(SslService::new(
            repo,
            ctx.audit.clone(),
            master_key,
            paths,
            challenge_server.clone(),
            acme_client,
            acme_endpoint,
            contact_email,
        ));
        let renewal_task = SslRenewalTask::new(service.clone());
        let migrations = vec![Migration {
            module: MODULE_NAME,
            version: "001".to_string(),
            description: "ssl initial schema".to_string(),
            sql: crate::migrations::SSL_V001.to_string(),
        }];
        Self {
            service,
            challenge_server,
            renewal_task,
            migrations,
        }
    }

    /// Return a clone of the shared service handle.
    pub fn service(&self) -> Arc<SslService> {
        self.service.clone()
    }

    /// Borrow the challenge server.
    pub fn challenge_server(&self) -> &AcmeHttpServer {
        &self.challenge_server
    }
}

impl Module for SslModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn background_tasks(
        &self,
        _ctx: &AppContext,
    ) -> Vec<Box<dyn openpanel_core::jobs::BackgroundTask>> {
        vec![Box::new(SslRenewalTask::new(self.service.clone()))]
    }
}
