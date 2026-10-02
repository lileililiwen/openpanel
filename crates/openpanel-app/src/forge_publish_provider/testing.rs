//! Double-backed provider harness for the suites that cannot inject
//! a port themselves.
//!
//! The forge publish provider is composed by the CLI entry point
//! from the docker engine. A suite that wants to exchange real
//! provider frames without a daemon composes the *same* graph over
//! the recording docker double through [`double_harness`], so the
//! integration and CLI suites exercise the production wiring instead
//! of a private re-implementation of it.

// Test-double setup may fail fast: an unbuildable double is a
// broken test environment, not a production condition. This mirrors
// the crate-level allow in `openpanel-test-support`.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use openpanel_domain::{
    ApplicationSource, ApplicationSourceKind, Email, EnvironmentKind, Password, Role, SecretSetRef,
    User, Username,
};

use crate::application_delivery::testing::noop_audit_sink;
use crate::application_delivery::{
    ApplicationDeliveryService, ConvergeRegistry, DeliveryTargetResolver, DockerBuildAdapter,
    DockerRuntimeAdapter,
};
use crate::deployment_adapters::{DeploymentAdapterService, DockerDeploymentAdapter};
use crate::docker::testing::{FakeDocker, FakeNetwork};

use super::{ForgePublishProvider, ForgePublishProviderConfig};

/// Registry namespace the harness composes with.
pub const HARNESS_REGISTRY_NAMESPACE: &str = "registry.example/openpanel";

/// The environment secret-set name the harness registers and the
/// composed manifest declares.
pub const HARNESS_SECRET_SET: &str = "prod-secrets";

/// The immutable digest the harness build adapter resolves.
pub const HARNESS_DIGEST: &str =
    "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

/// A composed provider plus the double behind it.
pub struct DoubleHarness {
    /// The provider under test.
    pub provider: Arc<ForgePublishProvider>,
    /// The recording docker double the provider runs on.
    pub docker: Arc<FakeDocker>,
    /// The delivery service the provider drives.
    delivery: Arc<ApplicationDeliveryService>,
    /// The authorised operator every frame is executed as.
    pub caller: User,
    /// The application a publish frame resolves by slug.
    pub application_name: String,
    /// The immutable digest the build adapter resolves.
    pub digest: String,
}

impl DoubleHarness {
    /// The delivery service backing the composed provider.
    pub fn delivery(&self) -> &Arc<ApplicationDeliveryService> {
        &self.delivery
    }
}

/// The operator a harness runs as: a fresh owner, so a suite never
/// depends on seeded identity state.
pub fn noop_operator() -> User {
    User::new(
        uuid::Uuid::new_v4(),
        Username::new("publisher").expect("static harness operator"),
        Email::new("publisher@example.com").expect("static harness operator"),
        Password::hash("correct horse battery staple").expect("static harness operator"),
        Role::Owner,
    )
}

/// Compose the production graph over a recording docker double and
/// register the application + environment a `forge:<slug>:
/// <environment>` publish frame names.
pub fn double_harness(
    caller: User,
    docker: Arc<FakeDocker>,
    application_name: &str,
    digest: &str,
) -> DoubleHarness {
    let runtime = Arc::new(
        DockerRuntimeAdapter::new(
            docker.clone(),
            Arc::new(FakeNetwork::new()),
            HARNESS_REGISTRY_NAMESPACE,
            Arc::new(ConvergeRegistry::new()),
        )
        .expect("harness runtime adapter is constructible"),
    );
    let build = Arc::new(
        DockerBuildAdapter::new(
            docker.clone(),
            HARNESS_REGISTRY_NAMESPACE.to_string(),
            8 * 1024 * 1024,
        )
        .expect("harness build adapter is constructible"),
    );
    let audit = noop_audit_sink();
    let delivery = Arc::new(ApplicationDeliveryService::new(
        build,
        runtime.clone(),
        audit.clone(),
    ));
    let source = ApplicationSource::new(
        ApplicationSourceKind::Oci,
        format!("oci://{HARNESS_REGISTRY_NAMESPACE}/{application_name}"),
        digest,
    )
    .expect("harness source is constructible");
    let application = delivery
        .register_application(&caller, application_name, source, "npm ci && npm run build")
        .expect("harness application registers");
    delivery
        .add_environment(
            &caller,
            application.id(),
            EnvironmentKind::Production,
            "production",
            4,
            SecretSetRef::new(HARNESS_SECRET_SET, None).expect("harness secret set"),
            None,
        )
        .expect("harness environment registers");
    let deployment = DockerDeploymentAdapter::new(
        runtime,
        Arc::new(DeliveryTargetResolver::new(
            delivery.clone(),
            caller.clone(),
        )),
    )
    .and_then(|adapter| adapter.with_declared_secret_refs(vec![HARNESS_SECRET_SET.to_string()]))
    .expect("harness deployment adapter is constructible");
    let adapters = Arc::new(DeploymentAdapterService::new(
        vec![Arc::new(deployment)],
        audit.clone(),
    ));
    let provider = Arc::new(ForgePublishProvider::new(
        ForgePublishProviderConfig {
            adapter_id: crate::DOCKER_DEPLOYMENT_ADAPTER_ID.to_string(),
            audit_actor: "forge-publish-provider".to_string(),
        },
        delivery.clone(),
        adapters,
        audit,
    ));
    DoubleHarness {
        provider,
        docker,
        delivery,
        caller,
        application_name: application_name.to_string(),
        digest: digest.to_string(),
    }
}
