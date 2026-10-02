//! CLI composition for the opt-in Forge stdio provider. All diagnostics are payload-free.
//!
//! The provider composes the production docker delivery adapters:
//! [`DockerBuildAdapter`] resolves a release digest, [`DockerRuntimeAdapter`]
//! converges the environment, and [`DockerDeploymentAdapter`] carries the
//! plan contract the forge facade runs. One runtime adapter instance backs
//! both delivery services so the build, runtime, and deployment adapters
//! agree on a single host target projection.
//!
//! Registry namespace, build-context ceiling, and optional push
//! credentials come from the `delivery` section of the generic
//! `Config.modules` map (env:
//! `OPENPANEL__MODULES__DELIVERY__REGISTRY_NAMESPACE`); no new top-level
//! config key is introduced.
use openpanel_app::{
    ApplicationDeliveryService, BollardDockerAdapter, ConvergeRegistry,
    DOCKER_DEPLOYMENT_ADAPTER_ID, DeliveryTargetResolver, DeploymentAdapterService, DockerAdapter,
    DockerBuildAdapter, DockerDeploymentAdapter, DockerRuntimeAdapter, ForgeProviderAudit,
    ForgeProviderProcessHandle, ForgePublishProvider, ForgePublishProviderConfig, NetworkAdapter,
    identity::repo::SqliteUserRepository, run_loop,
};
use openpanel_core::Config;
use openpanel_domain::{Role, UserRepository};
use std::sync::Arc;

/// Registry namespace used when the delivery section omits one.
const DEFAULT_REGISTRY_NAMESPACE: &str = "openpanel.local";

/// Default build-context ceiling (64 MiB) when the delivery section
/// omits one.
const DEFAULT_MAX_CONTEXT_BYTES: usize = 64 * 1024 * 1024;

/// Validated delivery settings read from the generic module config map.
struct DeliverySettings {
    registry_namespace: String,
    max_context_bytes: usize,
    registry_username: Option<String>,
    registry_password: Option<String>,
    /// Secret-set names a forge plan may reference. The plan
    /// contract matches secret references exactly against the
    /// manifest, and the names belong to the operator's
    /// environments, so the composition declares them.
    plan_secret_refs: Vec<String>,
}

fn refuse(reason: &str) -> ! {
    eprintln!("forge-provider: refusing startup ({reason})");
    std::process::exit(78)
}

/// Read the `delivery` section of `Config.modules`. A namespace that
/// would produce an invalid image reference is a startup refusal, not a
/// per-frame failure.
fn delivery_settings(config: &Config) -> DeliverySettings {
    let section = config.module_config("delivery").cloned();
    let text = |key: &str| {
        section
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    DeliverySettings {
        registry_namespace: text("registry_namespace")
            .unwrap_or_else(|| DEFAULT_REGISTRY_NAMESPACE.to_string()),
        max_context_bytes: section
            .as_ref()
            .and_then(|value| value.get("max_context_bytes"))
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_MAX_CONTEXT_BYTES),
        registry_username: text("registry_username"),
        registry_password: text("registry_password"),
        plan_secret_refs: section
            .as_ref()
            .and_then(|value| value.get("plan_secret_refs"))
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

pub(super) async fn serve(config: Arc<Config>) -> anyhow::Result<()> {
    let mut lifecycle = ForgeProviderProcessHandle::from_environment();
    if !lifecycle.is_provider_enabled() {
        refuse("provider disabled");
    }
    let operator = match std::env::var("OPENPANEL_FORGE_PUBLISH_OPERATOR") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => refuse("operator required"),
    };
    let (pool, audit, _db) = match super::bootstrap_persistence(&config).await {
        Ok(value) => value,
        Err(_) => refuse("persistence unavailable"),
    };
    let repo = SqliteUserRepository::new(pool);
    let caller = match repo.find_by_username(operator.trim()).await {
        Ok(Some(user)) => user,
        Ok(None) => refuse("unknown operator"),
        Err(_) => refuse("operator lookup failed"),
    };
    if !matches!(caller.role(), Role::Owner | Role::Admin) {
        refuse("operator requires Owner or Admin role");
    }
    if caller.disabled_at().is_some() {
        refuse("operator disabled");
    }
    let audit = Arc::new(ForgeProviderAudit::new(audit));
    let settings = delivery_settings(&config);
    // The engine client is composed once. A daemon that is down at
    // this moment surfaces as a typed delivery error per frame rather
    // than taking the provider down, so the transport stays available.
    let engine = match BollardDockerAdapter::connect_with_registry_credentials(
        settings.registry_username.clone(),
        settings.registry_password.clone(),
    ) {
        Ok(engine) => engine,
        Err(_) => refuse("docker engine unreachable"),
    };
    let engine = Arc::new(engine);
    let docker: Arc<dyn DockerAdapter> = engine.clone();
    let network: Arc<dyn NetworkAdapter> = engine;
    let build = match DockerBuildAdapter::new(
        docker.clone(),
        settings.registry_namespace.clone(),
        settings.max_context_bytes,
    ) {
        Ok(build) => build,
        Err(_) => refuse("delivery settings rejected"),
    };
    let runtime = match DockerRuntimeAdapter::new(
        docker,
        network,
        settings.registry_namespace,
        Arc::new(ConvergeRegistry::new()),
    ) {
        Ok(runtime) => Arc::new(runtime),
        Err(_) => refuse("delivery settings rejected"),
    };
    let delivery = Arc::new(ApplicationDeliveryService::new(
        Arc::new(build),
        runtime.clone(),
        audit.clone(),
    ));
    let deployment = match DockerDeploymentAdapter::new(
        runtime,
        Arc::new(DeliveryTargetResolver::new(
            delivery.clone(),
            caller.clone(),
        )),
    )
    .and_then(|adapter| adapter.with_declared_secret_refs(settings.plan_secret_refs.clone()))
    {
        Ok(adapter) => adapter,
        Err(_) => refuse("deployment adapter manifest rejected"),
    };
    let adapters = Arc::new(DeploymentAdapterService::new(
        vec![Arc::new(deployment)],
        audit.clone(),
    ));
    let provider = Arc::new(ForgePublishProvider::new(
        ForgePublishProviderConfig {
            adapter_id: DOCKER_DEPLOYMENT_ADAPTER_ID.to_string(),
            audit_actor: "forge-publish-provider".to_string(),
        },
        delivery,
        adapters,
        audit.clone(),
    ));
    std::panic::set_hook(Box::new(|_| eprintln!("forge-provider: handler panicked")));
    lifecycle.start_provider();
    run_loop(
        tokio::io::BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
        provider,
        Arc::new(caller),
        &mut lifecycle,
        audit,
    )
    .await?;
    Ok(())
}
