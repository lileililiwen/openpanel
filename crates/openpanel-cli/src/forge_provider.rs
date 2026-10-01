//! CLI composition for the opt-in Forge stdio provider. All diagnostics are payload-free.
use openpanel_app::{
    ApplicationDeliveryService, DeploymentAdapterService, ForgeProviderAudit,
    ForgeProviderProcessHandle, ForgePublishProvider, ForgePublishProviderConfig,
    UnavailableBuildAdapter, UnavailableRuntimeAdapter, identity::repo::SqliteUserRepository,
    run_loop,
};
use openpanel_core::Config;
use openpanel_domain::{Role, UserRepository};
use std::sync::Arc;

fn refuse(reason: &str) -> ! {
    eprintln!("forge-provider: refusing startup ({reason})");
    std::process::exit(78)
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
    let delivery = Arc::new(ApplicationDeliveryService::new(
        Arc::new(UnavailableBuildAdapter),
        Arc::new(UnavailableRuntimeAdapter),
        audit.clone(),
    ));
    let adapters = Arc::new(DeploymentAdapterService::new(vec![], audit.clone()));
    let provider = Arc::new(ForgePublishProvider::new(
        ForgePublishProviderConfig::default(),
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
