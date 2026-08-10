//! CLI command handlers. Each command bootstraps the application context
//! enough to call into the IdentityService or start the server.

use std::sync::Arc;

use anyhow::Context;
use base64::Engine;
use openpanel_api::build_router;
use openpanel_app::{
    AcmeEndpoint, BackupsModule, CronModule, DatabasesModule, DnsModule, FilesModule,
    IdentityModule, LogService, LogsModule, MailModule, MonitoringModule, SecurityModule,
    SecurityService, SitesModule, SoftwareCenterModule, SslModule, SslPaths, SystemServicesModule,
    databases::crypto as db_crypto,
};
use openpanel_core::{
    AppContext, Config, MigrationRunner, Module, SqliteAuditService, SqliteDriver,
};
use openpanel_domain::{
    Role,
    security::{FirewallRule, LoginKey, NetworkCidr, PortRange, Protocol, RuleAction},
};
use tokio::net::TcpListener;

/// Bootstraps persistence, runs all module migrations, and serves the OpenPanel
/// API + agent over HTTP on the configured bind address until the process exits.
pub async fn serve(config: Arc<Config>) -> anyhow::Result<()> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;

    let ctx = AppContext::new(config.clone(), db, audit.clone());

    // Architecture-owned audit schema
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .ok();

    let identity_module = IdentityModule::new(&ctx).await;
    let sites_module = SitesModule::new(&ctx).await;
    let master_key = load_master_key(&config)?;
    let databases_module = DatabasesModule::new(&ctx, master_key).await;
    let files_module = FilesModule::new(&ctx).await;
    let ssl_module = SslModule::new(&ctx, master_key, ssl_contact_email(&config)).await;
    let monitoring_module = MonitoringModule::new(&ctx).await;
    let cron_module = CronModule::new(&ctx).await;
    let backup_root = std::env::var("OPENPANEL__BACKUPS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/var/lib/openpanel/backups"));
    let backups_module = BackupsModule::with_root(
        &ctx,
        backup_root,
        Some(master_key),
        Some((cron_module.service(), std::path::PathBuf::from("/var/www"))),
    )
    .await;
    let log_root = std::env::var("OPENPANEL__LOGS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/var/log/openpanel"));
    let logs_module = LogsModule::with_root(&ctx, log_root).await;
    let security_module = SecurityModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let system_services_module = SystemServicesModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let dns_module = DnsModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mail_module = MailModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let software_center_module =
        SoftwareCenterModule::new(&ctx, sites_module.service(), databases_module.service())
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let runner = MigrationRunner::for_sqlite(pool.clone());
    runner
        .apply_module(identity_module.name(), &identity_module.migrations())
        .await
        .context("apply identity migrations")?;
    runner
        .apply_module(sites_module.name(), &sites_module.migrations())
        .await
        .context("apply sites migrations")?;
    runner
        .apply_module(databases_module.name(), &databases_module.migrations())
        .await
        .context("apply databases migrations")?;
    runner
        .apply_module(ssl_module.name(), &ssl_module.migrations())
        .await
        .context("apply ssl migrations")?;
    runner
        .apply_module(monitoring_module.name(), &monitoring_module.migrations())
        .await
        .context("apply monitoring migrations")?;
    runner
        .apply_module(cron_module.name(), &cron_module.migrations())
        .await
        .context("apply cron migrations")?;
    runner
        .apply_module(backups_module.name(), &backups_module.migrations())
        .await
        .context("apply backup migrations")?;
    runner
        .apply_module(logs_module.name(), &logs_module.migrations())
        .await
        .context("apply logs migrations")?;
    runner
        .apply_module(security_module.name(), &security_module.migrations())
        .await
        .context("apply security migrations")?;
    runner
        .apply_module(
            system_services_module.name(),
            &system_services_module.migrations(),
        )
        .await
        .context("apply system services migrations")?;
    runner
        .apply_module(dns_module.name(), &dns_module.migrations())
        .await
        .context("apply DNS migrations")?;
    runner
        .apply_module(mail_module.name(), &mail_module.migrations())
        .await
        .context("apply mail migrations")?;
    runner
        .apply_module(
            software_center_module.name(),
            &software_center_module.migrations(),
        )
        .await
        .context("apply software center migrations")?;

    let identity_svc = identity_module.service();
    let sites_svc = sites_module.service();
    let databases_svc = databases_module.service();
    let files_svc = files_module.service();
    let ssl_svc = ssl_module.service();
    let monitoring_svc = monitoring_module.service();
    let cron_svc = cron_module.service();
    let backups_svc = backups_module.service();
    let logs_svc = logs_module.service();
    let security_svc = security_module.service();
    let login_throttle = security_module.login_service();
    let system_services_svc = system_services_module.service();
    let dns_svc = dns_module.service();
    let mail_svc = mail_module.service();
    let software_center_svc = software_center_module.service();
    let app = build_router(
        identity_svc.clone(),
        sites_svc.clone(),
        databases_svc.clone(),
        files_svc.clone(),
        ssl_svc.clone(),
        monitoring_svc.clone(),
        cron_svc.clone(),
        backups_svc.clone(),
        logs_svc.clone(),
        security_svc.clone(),
        login_throttle.clone(),
        system_services_svc.clone(),
        dns_svc.clone(),
        mail_svc.clone(),
        software_center_svc.clone(),
    )
    .merge(openpanel_web::router(
        identity_svc,
        sites_svc,
        databases_svc,
        files_svc,
        ssl_svc,
        monitoring_svc,
        cron_svc,
        backups_svc,
        logs_svc,
        security_svc,
        login_throttle,
        system_services_svc,
        dns_svc,
        mail_svc,
        software_center_svc,
        web_runtime(&config, audit).with_capabilities(
            openpanel_web::layout::CapabilitySet::shipped()
                .with("cron")
                .with("backups")
                .with("logs")
                .with("host-security")
                .with("system-services")
                .with("dns")
                .with("mail")
                .with("software-center"),
        ),
    ));

    let addr = format!("{}:{}", config.server().bind, config.server().port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    tracing::info!(%addr, "openpanel listening on http://{addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn web_runtime(
    config: &Arc<Config>,
    audit: Arc<dyn openpanel_core::AuditService>,
) -> openpanel_web::WebRuntime {
    let database_path = config
        .database()
        .url
        .strip_prefix("sqlite://")
        .unwrap_or(&config.database().url);
    let data_path = std::path::Path::new(database_path)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("/var/lib/openpanel"))
        .to_path_buf();
    let config_path = std::env::var("OPENPANEL_CONFIG").unwrap_or_else(|_| {
        if std::path::Path::new("/etc/openpanel/openpanel.toml").exists() {
            "/etc/openpanel/openpanel.toml".to_string()
        } else {
            "./openpanel.toml".to_string()
        }
    });
    openpanel_web::WebRuntime::new(
        config.clone(),
        audit,
        data_path.join("web-preferences.json"),
        data_path.to_string_lossy().into_owned(),
        config_path,
    )
}

fn load_master_key(config: &Arc<Config>) -> anyhow::Result<[u8; 32]> {
    let raw = std::env::var("OPENPANEL__DATABASE__MASTER_KEY")
        .ok()
        .or_else(|| {
            config
                .module_config("database")
                .and_then(|v| v.get("master_key"))
                .and_then(|v| v.as_str().map(String::from))
        })
        .ok_or_else(|| {
            anyhow::anyhow!("OPENPANEL__DATABASE__MASTER_KEY must be set (base64-encoded 32 bytes)")
        })?;
    db_crypto::decode_master_key(&raw).map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Resolve the Let's Encrypt contact email from
/// `OPENPANEL__SSL__CONTACT_EMAIL` or fall back to the operator
/// account. Required by ACME.
fn ssl_contact_email(_config: &Arc<Config>) -> String {
    std::env::var("OPENPANEL__SSL__CONTACT_EMAIL")
        .unwrap_or_else(|_| "admin@openpanel.local".to_string())
}

async fn build_cron(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::CronService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let root = std::env::current_dir().context("resolve current directory")?;
    let module = CronModule::with_roots(&ctx, vec![root]).await;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

fn cron_owner() -> uuid::Uuid {
    uuid::Uuid::nil()
}

async fn build_backups(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::BackupService>> {
    let master_key = load_master_key(&config).ok();
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let root = std::env::var("OPENPANEL__BACKUPS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?.join("backups"));
    let cron_module = CronModule::with_roots(&ctx, vec![std::env::current_dir()?]).await;
    let runner = MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(cron_module.name(), &cron_module.migrations())
        .await?;
    let module = BackupsModule::with_root(
        &ctx,
        root,
        master_key,
        Some((cron_module.service(), std::env::current_dir()?)),
    )
    .await;
    runner
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}
fn backup_owner() -> uuid::Uuid {
    uuid::Uuid::nil()
}

async fn build_logs(config: Arc<Config>) -> anyhow::Result<Arc<LogService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let root = std::env::var("OPENPANEL__LOGS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/var/log/openpanel"));
    let module = LogsModule::with_root(&ctx, root).await;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

fn logs_actor() -> openpanel_app::logs::LogActor {
    openpanel_app::logs::LogActor::new(uuid::Uuid::nil(), openpanel_domain::Role::Owner)
}

async fn build_security(config: Arc<Config>) -> anyhow::Result<Arc<SecurityService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let module = SecurityModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

fn security_actor() -> uuid::Uuid {
    uuid::Uuid::nil()
}

async fn build_system_services(
    config: Arc<Config>,
) -> anyhow::Result<Arc<openpanel_app::ServiceManager>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let module = SystemServicesModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}
fn parse_service_action(
    value: &str,
) -> anyhow::Result<openpanel_domain::system_services::ServiceAction> {
    use openpanel_domain::system_services::ServiceAction;
    match value {
        "start" => Ok(ServiceAction::Start),
        "stop" => Ok(ServiceAction::Stop),
        "restart" => Ok(ServiceAction::Restart),
        "reload" => Ok(ServiceAction::Reload),
        "enable" => Ok(ServiceAction::Enable),
        "disable" => Ok(ServiceAction::Disable),
        _ => Err(anyhow::anyhow!("unknown service action")),
    }
}
/// List registered system services.
pub async fn services_list(config: Arc<Config>) -> anyhow::Result<()> {
    for value in build_system_services(config).await?.inventory().await? {
        println!(
            "{} {}",
            value.descriptor.id().as_str(),
            value.status.active_state
        );
    }
    Ok(())
}
/// Show one registered system service.
pub async fn services_status(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let value = build_system_services(config).await?.status(&id).await?;
    println!(
        "{} {}",
        value.descriptor.id().as_str(),
        value.status.active_state
    );
    Ok(())
}
/// Preview dependent capability impact.
pub async fn services_preview(
    config: Arc<Config>,
    id: String,
    action: String,
) -> anyhow::Result<()> {
    let value = build_system_services(config)
        .await?
        .preview(&id, parse_service_action(&action)?)?;
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}
/// Perform one typed local-Owner service action.
pub async fn services_action(
    config: Arc<Config>,
    id: String,
    action: String,
    confirmed: bool,
) -> anyhow::Result<()> {
    let value = build_system_services(config)
        .await?
        .perform(
            uuid::Uuid::nil(),
            Role::Owner,
            &id,
            parse_service_action(&action)?,
            confirmed,
        )
        .await?;
    println!("{}", value.status.active_state);
    Ok(())
}
/// Print bounded health history.
pub async fn services_history(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string(
            &build_system_services(config)
                .await?
                .history(&id, 100)
                .await?
        )?
    );
    Ok(())
}
/// Print bounded redacted journal entries.
pub async fn services_logs(config: Arc<Config>, id: String, limit: usize) -> anyhow::Result<()> {
    for line in build_system_services(config)
        .await?
        .logs(&id, limit)
        .await?
    {
        println!("{line}");
    }
    Ok(())
}

async fn build_dns(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::DnsService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let module = DnsModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}
fn parse_record_kind(value: &str) -> anyhow::Result<openpanel_domain::dns::RecordKind> {
    use openpanel_domain::dns::RecordKind;
    match value.to_ascii_uppercase().as_str() {
        "A" => Ok(RecordKind::A),
        "AAAA" => Ok(RecordKind::Aaaa),
        "CNAME" => Ok(RecordKind::Cname),
        "TXT" => Ok(RecordKind::Txt),
        "MX" => Ok(RecordKind::Mx),
        "CAA" => Ok(RecordKind::Caa),
        "NS" => Ok(RecordKind::Ns),
        "SRV" => Ok(RecordKind::Srv),
        _ => Err(anyhow::anyhow!("unsupported DNS record type")),
    }
}
/// Add a protected DNS provider account.
pub async fn dns_provider_add(
    config: Arc<Config>,
    kind: String,
    name: String,
    credential: String,
) -> anyhow::Result<()> {
    let account = build_dns(config)
        .await?
        .create_account(uuid::Uuid::nil(), Role::Owner, &kind, &name, &credential)
        .await?;
    println!("{} {}", account.id, account.name);
    Ok(())
}
/// List secret-free provider account metadata.
pub async fn dns_providers(config: Arc<Config>) -> anyhow::Result<()> {
    for account in build_dns(config).await?.accounts().await? {
        println!("{} {} {}", account.id, account.kind, account.name);
    }
    Ok(())
}
async fn first_dns_account(
    service: &openpanel_app::DnsService,
) -> anyhow::Result<openpanel_app::dns::ProviderAccount> {
    service
        .accounts()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no DNS provider account"))
}
/// Retest the first configured provider account.
pub async fn dns_provider_test(config: Arc<Config>) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    let tested = service
        .test_account(uuid::Uuid::nil(), Role::Owner, account.id)
        .await?;
    println!("{} tested", tested.id);
    Ok(())
}
/// Rotate the first provider account credential.
pub async fn dns_provider_rotate(config: Arc<Config>, credential: String) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    let rotated = service
        .rotate_account(uuid::Uuid::nil(), Role::Owner, account.id, &credential)
        .await?;
    println!("{} rotated", rotated.id);
    Ok(())
}
/// Enable or disable the first provider account.
pub async fn dns_provider_enabled(config: Arc<Config>, enabled: bool) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    let updated = service
        .set_account_enabled(uuid::Uuid::nil(), Role::Owner, account.id, enabled)
        .await?;
    println!("{} enabled={}", updated.id, updated.enabled);
    Ok(())
}
/// Delete first provider metadata after confirmation.
pub async fn dns_provider_delete(config: Arc<Config>, confirm: bool) -> anyhow::Result<()> {
    if !confirm {
        return Err(anyhow::anyhow!("DNS provider deletion requires --confirm"));
    }
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    service
        .delete_account(uuid::Uuid::nil(), Role::Owner, account.id)
        .await?;
    println!("deleted {}", account.id);
    Ok(())
}
/// Synchronize the first configured provider account.
pub async fn dns_sync(config: Arc<Config>) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    let result = service
        .sync(uuid::Uuid::nil(), Role::Owner, account.id)
        .await?;
    println!("zones={} records={}", result.zones, result.imported_records);
    Ok(())
}
/// List synchronized DNS zones.
pub async fn dns_zones(config: Arc<Config>) -> anyhow::Result<()> {
    for zone in build_dns(config).await?.zones().await? {
        println!("{} {}", zone.id, zone.name);
    }
    Ok(())
}
async fn dns_zone(
    service: &openpanel_app::DnsService,
    name: &str,
) -> anyhow::Result<openpanel_app::dns::ProviderZone> {
    service
        .zones()
        .await?
        .into_iter()
        .find(|zone| zone.name.as_str() == name.trim_end_matches('.').to_ascii_lowercase())
        .ok_or_else(|| anyhow::anyhow!("DNS zone not found"))
}
/// Add one typed DNS record.
pub async fn dns_record_add(
    config: Arc<Config>,
    zone: String,
    name: String,
    kind: String,
    value: String,
    ttl: u32,
) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    let record = service
        .create_record(
            uuid::Uuid::nil(),
            Role::Owner,
            zone.id,
            &name,
            parse_record_kind(&kind)?,
            &value,
            ttl,
            zone.remote_version.as_str(),
        )
        .await?;
    println!("{} {} {}", record.remote_id, record.name, record.data);
    Ok(())
}
/// Update one existing DNS record, preserving its type.
pub async fn dns_record_update(
    config: Arc<Config>,
    zone: String,
    record: String,
    value: String,
    ttl: u32,
) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    let found = service
        .records(zone.id)
        .await?
        .into_iter()
        .find(|item| item.name.as_str() == record.trim_end_matches('.').to_ascii_lowercase())
        .ok_or_else(|| anyhow::anyhow!("DNS record not found"))?;
    let updated = service
        .update_record(
            uuid::Uuid::nil(),
            Role::Owner,
            zone.id,
            &found.remote_id,
            found.name.as_str(),
            found.data.kind(),
            &value,
            ttl,
            found.remote_version.as_str(),
        )
        .await?;
    println!("{} {}", updated.name, updated.data);
    Ok(())
}
/// List synchronized records for a zone.
pub async fn dns_records(config: Arc<Config>, zone: String) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    for record in service.records(zone.id).await? {
        println!("{} {} {}", record.remote_id, record.name, record.data);
    }
    Ok(())
}
/// Check propagation status for a zone.
pub async fn dns_check(config: Arc<Config>, zone: String) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    let result = service.check(zone.id).await?;
    println!("{}", result.status);
    Ok(())
}
/// Delete exactly one matching record after confirmation.
pub async fn dns_record_delete(
    config: Arc<Config>,
    zone: String,
    record: String,
    confirm: bool,
) -> anyhow::Result<()> {
    if !confirm {
        return Err(anyhow::anyhow!("DNS record deletion requires --confirm"));
    }
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    let found = service
        .records(zone.id)
        .await?
        .into_iter()
        .find(|item| item.name.as_str() == record.trim_end_matches('.').to_ascii_lowercase())
        .ok_or_else(|| anyhow::anyhow!("DNS record not found"))?;
    service
        .delete_record(
            uuid::Uuid::nil(),
            Role::Owner,
            zone.id,
            &found.remote_id,
            found.remote_version.as_str(),
        )
        .await?;
    println!("deleted {}", found.remote_id);
    Ok(())
}

async fn build_mail(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::MailService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let module = MailModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

async fn mail_domain(
    service: &openpanel_app::MailService,
    name: &str,
) -> anyhow::Result<openpanel_app::mail::MailDomain> {
    service
        .domains(uuid::Uuid::nil(), Role::Owner)
        .await?
        .into_iter()
        .find(|domain| domain.name.as_str() == name.trim_end_matches('.').to_ascii_lowercase())
        .ok_or_else(|| anyhow::anyhow!("mail domain not found"))
}

/// Print dependency and DNS/TLS readiness without secrets.
pub async fn mail_readiness(config: Arc<Config>) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string(&build_mail(config).await?.readiness().await?)?
    );
    Ok(())
}

/// Create a disabled hosted-mail domain.
pub async fn mail_domain_add(config: Arc<Config>, name: String) -> anyhow::Result<()> {
    let domain = build_mail(config)
        .await?
        .create_domain(uuid::Uuid::nil(), Role::Owner, &name)
        .await?;
    println!("{} {}", domain.id, domain.name);
    Ok(())
}

/// List hosted-mail domains.
pub async fn mail_domains(config: Arc<Config>) -> anyhow::Result<()> {
    for domain in build_mail(config)
        .await?
        .domains(uuid::Uuid::nil(), Role::Owner)
        .await?
    {
        println!("{} {} enabled={}", domain.id, domain.name, domain.enabled);
    }
    Ok(())
}

/// Create a mailbox and print its one-time credential.
pub async fn mail_mailbox_add(
    config: Arc<Config>,
    domain: String,
    local: String,
    quota: u64,
    password: String,
) -> anyhow::Result<()> {
    let service = build_mail(config).await?;
    let domain = mail_domain(&service, &domain).await?;
    let quota = openpanel_domain::mail::MailQuota::new(quota, 1_024, 1_073_741_824)?;
    let credential = service
        .create_mailbox(
            uuid::Uuid::nil(),
            Role::Owner,
            domain.id,
            &local,
            quota,
            Some(&password),
        )
        .await?;
    println!("{} {}", credential.mailbox.address, credential.password);
    Ok(())
}

/// List secret-free mailboxes for a domain.
pub async fn mail_mailboxes(config: Arc<Config>, domain: String) -> anyhow::Result<()> {
    let service = build_mail(config).await?;
    let domain = mail_domain(&service, &domain).await?;
    for mailbox in service
        .mailboxes(uuid::Uuid::nil(), Role::Owner, domain.id)
        .await?
    {
        println!("{} quota={}", mailbox.address, mailbox.quota.bytes());
    }
    Ok(())
}

/// Create a non-cyclic alias.
pub async fn mail_alias_add(
    config: Arc<Config>,
    domain: String,
    source: String,
    destination: String,
) -> anyhow::Result<()> {
    let service = build_mail(config).await?;
    let domain = mail_domain(&service, &domain).await?;
    let alias = service
        .add_alias(
            uuid::Uuid::nil(),
            Role::Owner,
            domain.id,
            &source,
            &destination,
        )
        .await?;
    println!("{} -> {}", alias.source, alias.destination);
    Ok(())
}

/// Change a mailbox quota.
pub async fn mail_quota(config: Arc<Config>, address: String, bytes: u64) -> anyhow::Result<()> {
    let quota = openpanel_domain::mail::MailQuota::new(bytes, 1_024, 1_073_741_824)?;
    let mailbox = build_mail(config)
        .await?
        .update_quota(uuid::Uuid::nil(), Role::Owner, &address, quota)
        .await?;
    println!("{} quota={}", mailbox.address, mailbox.quota.bytes());
    Ok(())
}

/// Rotate and print a mailbox's one-time credential.
pub async fn mail_password(
    config: Arc<Config>,
    address: String,
    password: String,
) -> anyhow::Result<()> {
    let credential = build_mail(config)
        .await?
        .rotate_password(uuid::Uuid::nil(), Role::Owner, &address, &password)
        .await?;
    println!("{} {}", credential.mailbox.address, credential.password);
    Ok(())
}

/// Print aggregate mail diagnostics only.
pub async fn mail_status(config: Arc<Config>) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string(&build_mail(config).await?.status().await?)?
    );
    Ok(())
}

async fn build_software_center(
    config: Arc<Config>,
) -> anyhow::Result<Arc<openpanel_app::SoftwareCenterService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let sites_module = SitesModule::new(&ctx).await;
    let master_key = if std::env::var("OPENPANEL__SOFTWARE__ADAPTER").as_deref() == Ok("fake") {
        [0_u8; 32]
    } else {
        load_master_key(&ctx.config)?
    };
    let databases_module = DatabasesModule::new(&ctx, master_key).await;
    let module =
        SoftwareCenterModule::new(&ctx, sites_module.service(), databases_module.service())
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let runner = MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(sites_module.name(), &sites_module.migrations())
        .await?;
    runner
        .apply_module(databases_module.name(), &databases_module.migrations())
        .await?;
    runner
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

/// Print the trusted recovery catalog as secret-free JSON.
pub async fn software_catalog(config: Arc<Config>) -> anyhow::Result<()> {
    let catalog = build_software_center(config)
        .await?
        .catalog(Role::Owner)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&catalog)?);
    Ok(())
}

/// Discover catalog component lifecycle state as secret-free JSON.
pub async fn software_inventory(config: Arc<Config>) -> anyhow::Result<()> {
    let inventory = build_software_center(config)
        .await?
        .inventory(Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&inventory)?);
    Ok(())
}

/// Print an immutable installation preview as secret-free JSON.
pub async fn software_preview(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let preview = build_software_center(config)
        .await?
        .preview_install(uuid::Uuid::nil(), Role::Owner, &id)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}

/// Execute a persisted fresh preview by exact digest and one-use confirmation token.
pub async fn software_execute(
    config: Arc<Config>,
    digest: String,
    confirmation_token: String,
) -> anyhow::Result<()> {
    let job = build_software_center(config)
        .await?
        .execute(uuid::Uuid::nil(), Role::Owner, &digest, &confirmation_token)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&job)?);
    Ok(())
}

/// Preview and execute one catalog installation.
pub async fn software_install(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let service = build_software_center(config).await?;
    let preview = service
        .preview_install(uuid::Uuid::nil(), Role::Owner, &id)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let job = service
        .execute(
            uuid::Uuid::nil(),
            Role::Owner,
            preview.plan.digest(),
            &preview.confirmation_token,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&job)?);
    Ok(())
}

/// Preview and execute one adoption, update, or uninstall transaction.
pub async fn software_component_action(
    config: Arc<Config>,
    id: String,
    action: openpanel_app::software_center::ComponentAction,
) -> anyhow::Result<()> {
    let service = build_software_center(config).await?;
    let preview = service
        .preview_component(uuid::Uuid::nil(), Role::Owner, &id, action)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let job = service
        .execute(
            uuid::Uuid::nil(),
            Role::Owner,
            preview.plan.digest(),
            &preview.confirmation_token,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&job)?);
    Ok(())
}

/// Preview and execute one transactional WordPress or Drupal deployment.
pub async fn software_deploy(
    config: Arc<Config>,
    application: String,
    domain: String,
    php_version: String,
    locale: String,
) -> anyhow::Result<()> {
    let service = build_software_center(config).await?;
    let preview = service
        .preview_deployment(
            uuid::Uuid::nil(),
            Role::Owner,
            openpanel_app::software_center::ApplicationDeploymentInput {
                application,
                domain,
                php_version,
                locale,
                enable_dns: false,
                enable_tls: false,
                enable_backups: false,
            },
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let deployment = service
        .execute_deployment(
            uuid::Uuid::nil(),
            Role::Owner,
            preview.plan.digest(),
            &preview.confirmation_token,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&deployment)?);
    Ok(())
}

/// Print bounded Software Center jobs as secret-free JSON.
pub async fn software_jobs(config: Arc<Config>) -> anyhow::Result<()> {
    let jobs = build_software_center(config)
        .await?
        .jobs(Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&jobs)?);
    Ok(())
}

/// Request safe-checkpoint cancellation for an active job.
pub async fn software_cancel(config: Arc<Config>, job: String) -> anyhow::Result<()> {
    let job = build_software_center(config)
        .await?
        .cancel(uuid::Uuid::nil(), Role::Owner, uuid::Uuid::parse_str(&job)?)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&job)?);
    Ok(())
}

/// Generate a fresh confirmation for an interrupted transaction retry.
pub async fn software_retry(config: Arc<Config>, job: String) -> anyhow::Result<()> {
    let preview = build_software_center(config)
        .await?
        .retry_preview(uuid::Uuid::nil(), Role::Owner, uuid::Uuid::parse_str(&job)?)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}

/// Run recipe-supported rollback for an interrupted component installation.
pub async fn software_rollback(config: Arc<Config>, job: String) -> anyhow::Result<()> {
    let rolled_back = build_software_center(config)
        .await?
        .rollback_interrupted(uuid::Uuid::nil(), Role::Owner, uuid::Uuid::parse_str(&job)?)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&rolled_back)?);
    Ok(())
}

/// Print aggregate Software Center diagnostics as secret-free JSON.
pub async fn software_diagnostics(config: Arc<Config>) -> anyhow::Result<()> {
    let service = build_software_center(config).await?;
    let software = service
        .catalog_diagnostics(Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let jobs = service
        .jobs(Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let combined = serde_json::json!({
        "software": software,
        "jobs": jobs,
    });
    println!("{}", serde_json::to_string_pretty(&combined)?);
    Ok(())
}

/// Trigger a manual catalog refresh.
pub async fn software_refresh(config: Arc<Config>) -> anyhow::Result<()> {
    let outcome = build_software_center(config)
        .await?
        .refresh_catalog(uuid::Uuid::nil(), Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    Ok(())
}

/// Search the active catalog snapshot.
#[allow(clippy::too_many_arguments)]
pub async fn software_search(
    config: Arc<Config>,
    query: Option<String>,
    category: Option<String>,
    tag: Option<String>,
    installed_only: bool,
    update_available_only: bool,
    page: usize,
    page_size: usize,
) -> anyhow::Result<()> {
    use openpanel_app::software_center::CatalogQuery;
    let mut builder = CatalogQuery {
        page,
        page_size,
        ..CatalogQuery::default()
    };
    builder.text = query;
    if let Some(category) = category
        && let Ok(parsed) = category.parse()
    {
        builder.categories.push(parsed);
    }
    if let Some(tag) = tag
        && let Ok(parsed) = openpanel_domain::software_center::Tag::new(&tag)
    {
        builder.tags.push(parsed);
    }
    builder.installed_only = installed_only;
    builder.update_available_only = update_available_only;
    let page = build_software_center(config)
        .await?
        .search(Role::Owner, builder)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&page)?);
    Ok(())
}

/// Print a single catalog entry as secret-free JSON.
pub async fn software_show(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let entry = build_software_center(config)
        .await?
        .entry(Role::Owner, &id)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&entry)?);
    Ok(())
}

/// Report host-firewall adapter support.
pub async fn security_status(config: Arc<Config>) -> anyhow::Result<()> {
    let supported = build_security(config).await?.supported().await?;
    println!("supported={supported} table=inet openpanel");
    Ok(())
}

/// Add one validated enabled firewall rule.
pub async fn security_rule_add(
    config: Arc<Config>,
    protocol: String,
    port: u16,
    source: String,
    action: String,
    comment: String,
) -> anyhow::Result<()> {
    let protocol = match protocol.as_str() {
        "tcp" => Protocol::Tcp,
        "udp" => Protocol::Udp,
        _ => return Err(anyhow::anyhow!("protocol must be tcp or udp")),
    };
    let action = match action.as_str() {
        "allow" => RuleAction::Allow,
        "deny" => RuleAction::Deny,
        _ => return Err(anyhow::anyhow!("action must be allow or deny")),
    };
    let rule = FirewallRule::new(
        uuid::Uuid::new_v4(),
        protocol,
        PortRange::new(port, port)?,
        NetworkCidr::parse(&source)?,
        action,
        comment,
        true,
    )?;
    let id = rule.id();
    build_security(config).await?.save_rule(rule).await?;
    println!("created security rule {id}");
    Ok(())
}

/// List managed firewall rules.
pub async fn security_rule_list(config: Arc<Config>) -> anyhow::Result<()> {
    for rule in build_security(config).await?.rules().await? {
        println!(
            "{} {:?} {} {} {:?} {}",
            rule.id(),
            rule.protocol(),
            rule.ports(),
            rule.source(),
            rule.action(),
            rule.comment()
        );
    }
    Ok(())
}

/// Update one rule comment while retaining validated fields.
pub async fn security_rule_update(
    config: Arc<Config>,
    id: String,
    comment: String,
) -> anyhow::Result<()> {
    let service = build_security(config).await?;
    let id = uuid::Uuid::parse_str(&id)?;
    let old = service.rule(id).await?;
    let rule = FirewallRule::new(
        id,
        old.protocol(),
        old.ports(),
        old.source(),
        old.action(),
        comment,
        old.enabled(),
    )?;
    service.save_rule(rule).await?;
    println!("updated {id}");
    Ok(())
}

/// Enable or disable one rule.
pub async fn security_rule_enabled(
    config: Arc<Config>,
    id: String,
    enabled: bool,
) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_security(config)
        .await?
        .set_rule_enabled(id, enabled)
        .await?;
    println!("{} {id}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

/// Delete one rule.
pub async fn security_rule_delete(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_security(config).await?.delete_rule(id).await?;
    println!("deleted {id}");
    Ok(())
}

/// Print the complete nftables candidate.
pub async fn security_preview(config: Arc<Config>) -> anyhow::Result<()> {
    print!(
        "{}",
        build_security(config).await?.preview_saved(None).await?
    );
    Ok(())
}

/// Apply the complete saved candidate transactionally.
pub async fn security_apply(config: Arc<Config>) -> anyhow::Result<()> {
    build_security(config)
        .await?
        .apply_saved(security_actor(), None)
        .await?;
    println!("applied inet openpanel");
    Ok(())
}

/// Restore the last-known-good OpenPanel ruleset.
pub async fn security_rollback(config: Arc<Config>) -> anyhow::Result<()> {
    build_security(config)
        .await?
        .rollback(security_actor())
        .await?;
    println!("rolled back inet openpanel");
    Ok(())
}

/// List durable login blocks without account metadata beyond the normalized key.
pub async fn security_blocks(config: Arc<Config>) -> anyhow::Result<()> {
    for block in build_security(config).await?.blocks().await? {
        println!("{} {}", block.key().storage_key(), block.expires_at());
    }
    Ok(())
}

/// List canonical login address allowlists.
pub async fn security_allowlist_list(config: Arc<Config>) -> anyhow::Result<()> {
    for network in build_security(config).await?.allowlists().await? {
        println!("{network}");
    }
    Ok(())
}

/// Add a canonical login address allowlist.
pub async fn security_allowlist_add(config: Arc<Config>, network: String) -> anyhow::Result<()> {
    let network = NetworkCidr::parse(&network)?;
    build_security(config)
        .await?
        .add_allowlist(security_actor(), network)
        .await?;
    println!("allowlisted {network}");
    Ok(())
}

/// Delete a canonical login address allowlist.
pub async fn security_allowlist_delete(config: Arc<Config>, network: String) -> anyhow::Result<()> {
    let network = NetworkCidr::parse(&network)?;
    build_security(config)
        .await?
        .remove_allowlist(security_actor(), network)
        .await?;
    println!("removed {network}");
    Ok(())
}

/// End a login block; absence is idempotent for recovery scripts.
pub async fn security_unblock(config: Arc<Config>, key: String) -> anyhow::Result<()> {
    let service = build_security(config).await?;
    let key = LoginKey::stored(&key)?;
    if service.unblock(security_actor(), key).await.is_ok() {
        println!("unblocked");
    } else {
        println!("already unblocked");
    }
    Ok(())
}

/// List registered log sources visible to the local Owner CLI.
pub async fn logs_sources(config: Arc<Config>) -> anyhow::Result<()> {
    let service = build_logs(config).await?;
    for source in service.sources(logs_actor()).await? {
        println!("{} {}", source.name(), source.id());
    }
    Ok(())
}

/// Print newest redacted entries from a registered source.
pub async fn logs_tail(config: Arc<Config>, source: String, limit: usize) -> anyhow::Result<()> {
    let service = build_logs(config).await?;
    let source = service.resolve(logs_actor(), &source).await?;
    let page = service
        .entries(
            logs_actor(),
            openpanel_app::logs::LogReadQuery::new(source.id(), limit),
        )
        .await?;
    for entry in page.entries {
        println!("{}", entry.text);
    }
    Ok(())
}

/// Print retained traffic aggregates as JSON.
pub async fn logs_traffic(config: Arc<Config>) -> anyhow::Result<()> {
    let rows = build_logs(config).await?.traffic(logs_actor()).await?;
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

/// Print recent audit events as JSON.
pub async fn logs_audit(config: Arc<Config>) -> anyhow::Result<()> {
    let events = build_logs(config)
        .await?
        .audit_events(logs_actor(), 100)
        .await?;
    println!("{}", serde_json::to_string_pretty(&events)?);
    Ok(())
}

/// Write a bounded redacted log export to an explicit destination.
pub async fn logs_export(
    config: Arc<Config>,
    source: String,
    output: std::path::PathBuf,
) -> anyhow::Result<()> {
    let service = build_logs(config).await?;
    let source = service.resolve(logs_actor(), &source).await?;
    let bytes = service
        .download(
            logs_actor(),
            source.id(),
            openpanel_app::logs::MAX_DOWNLOAD_BYTES,
        )
        .await?;
    std::fs::write(&output, bytes).with_context(|| format!("write {}", output.display()))?;
    println!("exported {}", output.display());
    Ok(())
}
/// Create a backup plan.
pub async fn backup_plan_create(
    config: Arc<Config>,
    name: String,
    schedule: String,
    timezone: String,
    panel_metadata: bool,
    retention: usize,
) -> anyhow::Result<()> {
    if !panel_metadata {
        return Err(anyhow::anyhow!("select at least one resource"));
    }
    let plan = build_backups(config)
        .await?
        .create_plan(
            backup_owner(),
            openpanel_app::backups::BackupPlanInput {
                name,
                resources: vec![openpanel_domain::backups::BackupResource::PanelMetadata],
                schedule,
                timezone,
                retention_copies: retention,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("created backup plan {}", plan.id());
    Ok(())
}
/// List backup plans.
pub async fn backup_plan_list(config: Arc<Config>) -> anyhow::Result<()> {
    for plan in build_backups(config)
        .await?
        .plans(backup_owner(), false)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!("{}  {}  {}", plan.id(), plan.name(), plan.schedule());
    }
    Ok(())
}
/// Show a backup plan.
pub async fn backup_plan_get(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let plan = build_backups(config)
        .await?
        .plan(backup_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}  {}", plan.id(), plan.name());
    Ok(())
}
/// Update backup retention.
pub async fn backup_plan_update(
    config: Arc<Config>,
    id: String,
    retention: usize,
) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .update_plan(
            backup_owner(),
            false,
            id,
            openpanel_app::backups::BackupPlanUpdate {
                name: None,
                retention_copies: Some(retention),
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("updated {id}");
    Ok(())
}
/// Enable or disable a backup plan.
pub async fn backup_plan_enabled(
    config: Arc<Config>,
    id: String,
    enabled: bool,
) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .set_enabled(backup_owner(), false, id, enabled)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("changed {id}");
    Ok(())
}
/// Delete a backup plan.
pub async fn backup_plan_delete(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .delete_plan(backup_owner(), false, id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}
/// Run a backup plan immediately.
pub async fn backup_run(config: Arc<Config>, plan_id: String) -> anyhow::Result<()> {
    let run = build_backups(config)
        .await?
        .run_plan(backup_owner(), false, uuid::Uuid::parse_str(&plan_id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("backup run {}", run.id());
    Ok(())
}
/// List backup runs.
pub async fn backup_runs(config: Arc<Config>) -> anyhow::Result<()> {
    for run in build_backups(config)
        .await?
        .runs(backup_owner(), false)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!("{}  {:?}", run.id(), run.state());
    }
    Ok(())
}
/// Show backup status.
pub async fn backup_status(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let run = build_backups(config)
        .await?
        .run(backup_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}  {:?}", run.id(), run.state());
    Ok(())
}
/// Verify a backup.
pub async fn backup_verify(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .verify(backup_owner(), false, id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("verified {id}");
    Ok(())
}
/// Preview a restore.
pub async fn backup_restore_preview(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let preview = build_backups(config)
        .await?
        .restore_preview(backup_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("ready={} bytes={}", preview.ready, preview.required_bytes);
    Ok(())
}
/// Start a safe restore.
pub async fn backup_restore_start(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let job = build_backups(config)
        .await?
        .restore(
            backup_owner(),
            false,
            uuid::Uuid::parse_str(&id)?,
            openpanel_app::backups::RestoreInput {
                resources: vec![],
                conflict_policy: "fail".into(),
                confirmation_token: None,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("restore {}", job.id);
    Ok(())
}
/// Delete a finalized backup run.
pub async fn backup_delete(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .delete_run(backup_owner(), false, id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}

/// Create a direct command cron job.
#[allow(clippy::too_many_arguments)]
pub async fn cron_create(
    config: Arc<Config>,
    name: String,
    schedule: String,
    timezone: String,
    executable: String,
    arguments: Vec<String>,
    working_directory: String,
    timeout: u64,
) -> anyhow::Result<()> {
    let svc = build_cron(config).await?;
    let job = svc
        .create(
            cron_owner(),
            openpanel_app::cron::CronInput {
                name,
                schedule,
                timezone,
                kind: "command".into(),
                executable: Some(executable),
                arguments,
                working_directory: Some(working_directory),
                url: None,
                method: None,
                timeout_secs: timeout,
                overlap_policy: "skip".into(),
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("created cron job {}", job.id());
    Ok(())
}

/// List cron jobs.
pub async fn cron_list(config: Arc<Config>) -> anyhow::Result<()> {
    for job in build_cron(config)
        .await?
        .list(cron_owner(), false)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!(
            "{}  {}  {} {}",
            job.id(),
            job.name(),
            job.schedule().expression(),
            job.schedule().timezone()
        );
    }
    Ok(())
}
/// Show a cron job.
pub async fn cron_get(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let job = build_cron(config)
        .await?
        .get(cron_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}  {}  {}",
        job.id(),
        job.name(),
        job.schedule().expression()
    );
    Ok(())
}
/// Change a cron expression.
pub async fn cron_update(config: Arc<Config>, id: String, schedule: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_cron(config)
        .await?
        .update(
            cron_owner(),
            false,
            id,
            openpanel_app::cron::CronUpdate {
                name: None,
                schedule: Some(schedule),
                timezone: None,
                timeout_secs: None,
                overlap_policy: None,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("updated {id}");
    Ok(())
}
/// Enable or disable a cron job.
pub async fn cron_enabled(config: Arc<Config>, id: String, enabled: bool) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_cron(config)
        .await?
        .set_enabled(cron_owner(), false, id, enabled)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{} {id}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}
/// Execute a cron job now.
pub async fn cron_run(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let run = build_cron(config)
        .await?
        .run_now(cron_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("run {} {:?}", run.id(), run.state());
    Ok(())
}
/// List cron execution history.
pub async fn cron_runs(config: Arc<Config>, job_id: Option<String>) -> anyhow::Result<()> {
    let job_id = job_id.map(|id| uuid::Uuid::parse_str(&id)).transpose()?;
    for run in build_cron(config)
        .await?
        .runs(cron_owner(), false, job_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!("{}  {}  {:?}", run.id(), run.job_id(), run.state());
    }
    Ok(())
}
/// Delete a cron job.
pub async fn cron_delete(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_cron(config)
        .await?
        .delete(cron_owner(), false, id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}

// Suppress unused-import warning when the env helpers below are
// pruned. Kept here so future flags (e.g. staging/production
// override) have a place to land.
#[allow(dead_code)]
const _: Option<AcmeEndpoint> = None;
#[allow(dead_code)]
const _: Option<SslPaths> = None;

/// Applies pending identity and sites migrations, then exits without starting the server.
pub async fn migrate(config: Arc<Config>) -> anyhow::Result<()> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);

    let identity_module = IdentityModule::new(&ctx).await;
    let sites_module = SitesModule::new(&ctx).await;

    let runner = MigrationRunner::for_sqlite(pool.clone());
    runner
        .apply_module(identity_module.name(), &identity_module.migrations())
        .await?;
    runner
        .apply_module(sites_module.name(), &sites_module.migrations())
        .await?;
    println!("migrations applied");
    Ok(())
}

/// Creates a new user with the given username, email, password, and role.
pub async fn create_user(
    config: Arc<Config>,
    username: String,
    email: String,
    password: String,
    role: Role,
) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let user = svc
        .create_user(&username, &email, &password, role, "cli")
        .await?;
    println!("created user {} ({})", user.username(), user.id());
    Ok(())
}

/// Lists all users (id, username, email, role) to stdout.
pub async fn list_users(config: Arc<Config>) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    for user in svc.list_users().await? {
        println!(
            "{}  {}  <{}>  {}",
            user.id(),
            user.username(),
            user.email(),
            user.role()
        );
    }
    Ok(())
}

/// Disables the user identified by the given UUID, preventing future logins.
pub async fn disable_user(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    svc.disable_user(uuid, "cli").await?;
    println!("disabled {id}");
    Ok(())
}

/// Deletes the user identified by the given UUID.
pub async fn delete_user(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    svc.delete_user(uuid, "cli").await?;
    println!("deleted {id}");
    Ok(())
}

/// Creates a new site with the given primary domain, owner, aliases, and PHP options.
pub async fn create_site(
    config: Arc<Config>,
    domain: String,
    owner_username: String,
    aliases: Vec<String>,
    php: bool,
    php_version: Option<String>,
    document_root: Option<String>,
) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin" || u.username().as_str() == owner_username)
        .ok_or_else(|| anyhow::anyhow!("caller user not found"))?;
    let owner = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == owner_username)
        .ok_or_else(|| anyhow::anyhow!("owner user `{owner_username}` not found"))?;
    let site = sites_svc
        .create_site(
            &caller,
            owner.id(),
            &domain,
            aliases,
            php,
            php_version,
            document_root,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("created site {} ({})", site.primary_domain(), site.id());
    Ok(())
}

/// Lists all sites (id, primary domain, status, owner id) to stdout.
pub async fn list_sites(config: Arc<Config>) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    for site in sites_svc
        .list_sites(&caller)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!(
            "{}  {}  {}  owner={}",
            site.id(),
            site.primary_domain(),
            site.status(),
            site.owner_id()
        );
    }
    Ok(())
}

/// Deletes the site identified by the given UUID.
pub async fn delete_site(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    sites_svc
        .delete_site(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}

/// Re-enables the site identified by the given UUID.
pub async fn enable_site(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    sites_svc
        .enable_site(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("enabled {id}");
    Ok(())
}

/// Disables the site identified by the given UUID (stops serving traffic without deletion).
pub async fn disable_site(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    sites_svc
        .disable_site(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("disabled {id}");
    Ok(())
}

async fn bootstrap_persistence(
    config: &Arc<Config>,
) -> anyhow::Result<(
    sqlx::Pool<sqlx::Sqlite>,
    Arc<SqliteAuditService>,
    Arc<dyn openpanel_core::DatabaseDriver>,
)> {
    let driver = SqliteDriver::new(config.database().url.clone());
    let pool = driver.connect().await.context("connect sqlite")?;
    SqliteAuditService::new(pool.clone())
        .ensure_schema()
        .await
        .context("ensure audit schema")?;
    let audit = Arc::new(SqliteAuditService::new(pool.clone()));
    let db: Arc<dyn openpanel_core::DatabaseDriver> = Arc::new(driver);
    Ok((pool, audit, db))
}

async fn build_identity(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::IdentityService>,
    Arc<SqliteAuditService>,
    sqlx::Pool<sqlx::Sqlite>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit.clone());
    let module = IdentityModule::new(&ctx).await;
    Ok((module.service(), audit, pool))
}

async fn build_sites(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::SitesService>,
    Arc<openpanel_app::IdentityService>,
    Arc<SqliteAuditService>,
    sqlx::Pool<sqlx::Sqlite>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit.clone());
    let identity_module = IdentityModule::new(&ctx).await;
    let sites_module = SitesModule::new(&ctx).await;
    Ok((
        sites_module.service(),
        identity_module.service(),
        audit,
        pool,
    ))
}

async fn build_databases(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::DatabasesService>,
    Arc<openpanel_app::IdentityService>,
    Arc<SqliteAuditService>,
    sqlx::Pool<sqlx::Sqlite>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit.clone());
    let identity_module = IdentityModule::new(&ctx).await;
    let master_key = load_master_key(&build_helper_config(&pool, &audit))?;
    let databases_module = DatabasesModule::new(&ctx, master_key).await;
    Ok((
        databases_module.service(),
        identity_module.service(),
        audit,
        pool,
    ))
}

// Helper that re-derives an Arc<Config> from the persistence layer.
// Since we already have the live config in callers, this is just a
// placeholder — the master key actually comes from OPENPANEL__DATABASE__MASTER_KEY
// env var. The returned `Arc<Config>` is unused.
fn build_helper_config(
    _pool: &sqlx::Pool<sqlx::Sqlite>,
    _audit: &Arc<SqliteAuditService>,
) -> Arc<Config> {
    Arc::new(Config::default())
}

/// Creates a new MySQL database and DB user owned by the given user.
/// Prints the generated password to stdout (it will not be shown again).
pub async fn create_database(
    config: Arc<Config>,
    owner_username: String,
    suffix: String,
    charset: Option<String>,
) -> anyhow::Result<()> {
    let (databases_svc, identity_svc, _audit, _pool) = build_databases(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin" || u.username().as_str() == owner_username)
        .ok_or_else(|| anyhow::anyhow!("caller user not found"))?;
    let owner = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == owner_username)
        .ok_or_else(|| anyhow::anyhow!("owner user `{owner_username}` not found"))?;
    let (db, password) = databases_svc
        .create_database(&caller, owner.id(), &owner_username, &suffix, charset)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "created database {} (id={})\n  db_user: {}\n  password: {}\n  -> copy this password into your site config; it will not be shown again.",
        db.name(),
        db.id(),
        db.db_user(),
        password
    );
    Ok(())
}

/// Lists all databases (id, name, status, owner id) to stdout.
pub async fn list_databases(config: Arc<Config>) -> anyhow::Result<()> {
    let (databases_svc, identity_svc, _audit, _pool) = build_databases(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    for db in databases_svc
        .list_databases(&caller)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!(
            "{}  {}  {}  owner={}",
            db.id(),
            db.name(),
            db.status(),
            db.owner_id()
        );
    }
    Ok(())
}

/// Deletes the database identified by the given UUID.
pub async fn delete_database(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (databases_svc, identity_svc, _audit, _pool) = build_databases(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid database id")?;
    databases_svc
        .delete_database(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}

/// Rotates the DB user's password for the database identified by the given UUID.
/// Prints the new password to stdout (it will not be shown again).
pub async fn change_database_password(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (databases_svc, identity_svc, _audit, _pool) = build_databases(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid database id")?;
    let new_password = databases_svc
        .change_password(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "rotated password for {id}\n  new password: {new_password}\n  -> copy into your site config; it will not be shown again."
    );
    Ok(())
}

async fn build_files(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::FilesService>,
    Arc<openpanel_app::SitesService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (_pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit.clone());
    let identity_module = IdentityModule::new(&ctx).await;
    let sites_module = SitesModule::new(&ctx).await;
    let files_module = FilesModule::new(&ctx).await;
    Ok((
        files_module.service(),
        sites_module.service(),
        identity_module.service(),
    ))
}

async fn resolve_site_id(
    _sites_svc: &openpanel_app::SitesService,
    site: &str,
) -> anyhow::Result<uuid::Uuid> {
    // Try UUID first
    if let Ok(id) = uuid::Uuid::parse_str(site) {
        return Ok(id);
    }
    // Try domain — use the service's own caller resolution
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.ok();
    let _ = pool;
    anyhow::bail!("site id `{site}` not a UUID; pass the UUID instead")
}

/// Lists entries under `path` inside the site's document root as a tabular stdout dump.
pub async fn file_list(config: Arc<Config>, site: String, path: String) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(if path.is_empty() { "" } else { &path })
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let entries = files_svc
        .list_dir(&caller, site_id, &rel)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{:<8} {:>10}  {:<6}  NAME", "TYPE", "SIZE", "MODE");
    for e in entries {
        let t = if e.is_dir { "dir" } else { "file" };
        println!("{:<8} {:>10}  {:<6}  {}", t, e.size, e.mode, e.name);
    }
    Ok(())
}

/// Writes the raw bytes of the file at `path` inside the site's document root to stdout.
pub async fn file_read(config: Arc<Config>, site: String, path: String) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(if path.is_empty() { "" } else { &path })
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let (bytes, _mtime) = files_svc
        .read_file(&caller, site_id, &rel)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    use std::io::Write;
    std::io::stdout().write_all(&bytes)?;
    Ok(())
}

/// Writes `content` to the file at `path` inside the site's document root.
pub async fn file_write(
    config: Arc<Config>,
    site: String,
    path: String,
    content: String,
) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(&path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    files_svc
        .write_file(&caller, site_id, &rel, content.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {} bytes to {path}", content.len());
    Ok(())
}

/// Creates a directory at `path` inside the site's document root.
pub async fn file_mkdir(config: Arc<Config>, site: String, path: String) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(&path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    files_svc
        .mkdir(&caller, site_id, &rel)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("created {path}");
    Ok(())
}

/// Removes the file or directory at `path` inside the site's document root.
/// If `recursive` is true, non-empty directories are removed as well.
pub async fn file_rm(
    config: Arc<Config>,
    site: String,
    path: String,
    recursive: bool,
) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(&path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    files_svc
        .remove(&caller, site_id, &rel, recursive)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("removed {path}");
    Ok(())
}

/// Renames/moves `from` to `to` within the site's document root.
pub async fn file_rename(
    config: Arc<Config>,
    site: String,
    from: String,
    to: String,
) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let from_rel = openpanel_domain::files::path::Path::new(&from)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let to_rel = openpanel_domain::files::path::Path::new(&to)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    files_svc
        .rename(&caller, site_id, &from_rel, &to_rel)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("renamed {from} -> {to}");
    Ok(())
}

/// Changes the permission bits of the file at `path` inside the site's document root.
/// `mode` is an octal string (e.g. `"755"` or `"0644"`).
pub async fn file_chmod(
    config: Arc<Config>,
    site: String,
    path: String,
    mode: String,
) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(&path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let m = u32::from_str_radix(mode.trim_start_matches('0'), 8)
        .map_err(|e| anyhow::anyhow!("invalid octal `{mode}`: {e}"))?;
    files_svc
        .chmod(&caller, site_id, &rel, m)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("chmod {mode} {path}");
    Ok(())
}

/// Bootstrap the `SslService` for CLI subcommands. Uses the path
/// overrides from `OPENPANEL__SSL__PATHS__CERT_DIR` /
/// `OPENPANEL__SSL__PATHS__KEY_DIR` if set (so tests can redirect
/// the cert/key writes to a sandbox), otherwise falls back to
/// `/etc/openpanel/ssl/certs` and `/etc/openpanel/ssl/keys`.
async fn build_ssl(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::SslService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = openpanel_core::AppContext::new(config.clone(), db, audit);
    let master_key = load_master_key(&config)?;
    let paths = resolve_ssl_paths();
    let module = openpanel_app::SslModule::with_paths(
        &ctx,
        paths,
        openpanel_app::AcmeEndpoint::default_safe(),
        master_key,
        ssl_contact_email(&config),
    )
    .await;
    let runner = openpanel_core::MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply ssl migrations")?;
    Ok(module.service())
}

fn resolve_ssl_paths() -> openpanel_app::SslPaths {
    let cert_dir = std::env::var("OPENPANEL__SSL__PATHS__CERT_DIR").ok();
    let key_dir = std::env::var("OPENPANEL__SSL__PATHS__KEY_DIR").ok();
    match (cert_dir, key_dir) {
        (Some(cert_dir), Some(key_dir)) => openpanel_app::SslPaths {
            cert_dir: std::path::PathBuf::from(cert_dir),
            key_dir: std::path::PathBuf::from(key_dir),
        },
        _ => openpanel_app::SslPaths::default_paths(),
    }
}

/// Pretty-print a `Certificate` (metadata only — never the key) for
/// CLI output.
fn print_certificate_row(cert: &openpanel_domain::Certificate) {
    println!(
        "{domain:<40} {source:<12} {status:<10} issuer={issuer:<24} valid_to={valid_to}",
        domain = cert.domain,
        source = cert.source.as_str(),
        status = format!("{:?}", cert.status(cert.valid_to)).to_lowercase(),
        issuer = cert.issuer,
        valid_to = cert.valid_to.to_rfc3339(),
    );
}

/// `openpanel ssl list` — print every certificate's metadata.
pub async fn ssl_list(config: Arc<Config>) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    let certs = svc
        .list()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if certs.is_empty() {
        println!("(no certificates)");
        return Ok(());
    }
    for cert in certs {
        print_certificate_row(&cert);
    }
    Ok(())
}

/// `openpanel ssl issue <domain> [--production]` — ACME HTTP-01.
///
/// By default targets the staging endpoint so fresh installs don't
/// burn Let's Encrypt rate limits; pass `--production` to switch.
pub async fn ssl_issue(
    config: Arc<Config>,
    domain: String,
    production: bool,
) -> anyhow::Result<()> {
    let endpoint = if production {
        openpanel_app::AcmeEndpoint::Production
    } else {
        openpanel_app::AcmeEndpoint::Staging
    };
    eprintln!(
        "[ssl] using ACME endpoint: {} (override at the module level \
         if you need a different CA)",
        endpoint.as_str()
    );
    // The service holds the configured endpoint at construction
    // time. To honor the CLI flag we'd need to rebuild the module
    // here — for v0.1 we rebuild with the requested endpoint.
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = openpanel_core::AppContext::new(config.clone(), db, audit);
    let master_key = load_master_key(&config)?;
    let paths = openpanel_app::SslPaths::default_paths();
    let acme: Arc<dyn openpanel_app::ssl::acme::AcmeClient> = Arc::new(
        openpanel_app::ssl::acme::RustlsAcmeClient::new(endpoint, ssl_contact_email(&config)),
    );
    let svc = Arc::new(openpanel_app::SslService::new(
        Arc::new(openpanel_app::ssl::SqliteCertificateRepository::new(pool)),
        ctx.audit.clone(),
        master_key,
        paths,
        openpanel_app::AcmeHttpServer::new(),
        acme,
        endpoint,
        ssl_contact_email(&config),
    ));
    let cert = svc
        .issue_acme(&domain)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("issued:");
    print_certificate_row(&cert);
    Ok(())
}

/// `openpanel ssl upload <domain> --cert <pem> [--chain <pem>]
/// --key <pem>` — manual PEM upload.
pub async fn ssl_upload(
    config: Arc<Config>,
    domain: String,
    cert: String,
    chain: Option<String>,
    key: String,
) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    let cert_pem = std::fs::read_to_string(&cert).context("read cert pem")?;
    let chain_pem = match chain {
        Some(p) => std::fs::read_to_string(&p).context("read chain pem")?,
        None => String::new(),
    };
    let key_pem = std::fs::read_to_string(&key).context("read key pem")?;
    let cert = svc
        .upload_manual(&domain, &cert_pem, &chain_pem, &key_pem)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("uploaded:");
    print_certificate_row(&cert);
    Ok(())
}

/// `openpanel ssl self-signed <domain>` — 365-day self-signed.
pub async fn ssl_self_signed(config: Arc<Config>, domain: String) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    let cert = svc
        .generate_self_signed(&domain, 365)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("generated:");
    print_certificate_row(&cert);
    Ok(())
}

/// `openpanel ssl revoke <domain>` — revoke + delete.
pub async fn ssl_revoke(config: Arc<Config>, domain: String) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    svc.delete(&domain)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("revoked {domain}");
    Ok(())
}

/// `openpanel ssl renew <domain>` — force-renew (ACME only).
pub async fn ssl_renew(config: Arc<Config>, domain: String) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    let cert = svc
        .renew_now(&domain)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("renewed:");
    print_certificate_row(&cert);
    Ok(())
}

/// Bootstrap the `MonitoringService` for CLI subcommands.
async fn build_monitoring(
    config: Arc<Config>,
) -> anyhow::Result<Arc<openpanel_app::MonitoringService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = openpanel_core::AppContext::new(config.clone(), db, audit);
    let module = MonitoringModule::new(&ctx).await;
    let runner = openpanel_core::MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply monitoring migrations")?;
    Ok(module.service())
}

/// `openpanel dev` — zero-config launcher: pick a writable data dir,
/// run migrations, bootstrap a default owner if none exists, then start
/// the web + API server. Idempotent.
pub async fn dev_up(config: Arc<Config>) -> anyhow::Result<()> {
    let (data_dir, db_url, master_key) = ensure_dev_defaults(&config)?;
    println!("openpanel dev");
    println!("  data dir:   {}", data_dir.display());
    println!("  database:   {db_url}");
    println!(
        "  bind:       {}:{}",
        config.server().bind,
        config.server().port
    );

    // Build a config that reflects the dev defaults without going back
    // through `Config::load` (which would re-read the same env vars
    // we just consumed). The defaults are layered on top of whatever
    // the user already passed via env / file / CLI.
    let config = build_dev_config(config, &db_url, &master_key);

    // 1. Migrations (idempotent: apply_module skips applied ones).
    migrate(config.clone())
        .await
        .context("apply dev migrations")?;

    // 2. Bootstrap default owner if the users table is empty.
    bootstrap_default_owner(&config).await?;

    println!();
    println!(
        "→ Login at http://{}:{}/login",
        config.server().bind,
        config.server().port
    );
    println!("  user:     admin");
    println!("  password: openpanel-dev  (CHANGE THIS IMMEDIATELY in production)");

    // 3. Serve.
    serve(config).await
}

/// Resolve the dev data directory, SQLite URL, and master key:
/// * data dir: `$OPENPANEL_DATA_DIR` or `/tmp/openpanel-dev` (created
///   if missing)
/// * db url:   `$OPENPANEL__DATABASE__URL` or `<data_dir>/openpanel.db`
/// * master key: `$OPENPANEL__DATABASE__MASTER_KEY` if set, else
///   generated and persisted to `<data_dir>/master.key`
fn ensure_dev_defaults(
    _config: &Arc<Config>,
) -> anyhow::Result<(std::path::PathBuf, String, String)> {
    let data_dir = std::env::var("OPENPANEL_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/openpanel-dev"));
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("create data dir {}", data_dir.display()))?;

    let db_url = std::env::var("OPENPANEL__DATABASE__URL")
        .unwrap_or_else(|_| format!("sqlite://{}/openpanel.db", data_dir.display()));

    let master_key = match std::env::var("OPENPANEL__DATABASE__MASTER_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            let key_path = data_dir.join("master.key");
            if key_path.exists() {
                std::fs::read_to_string(&key_path)
                    .with_context(|| format!("read {}", key_path.display()))?
            } else {
                let mut bytes = [0u8; 32];
                use rand::RngCore;
                rand::rngs::OsRng.fill_bytes(&mut bytes);
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                std::fs::write(&key_path, &encoded)
                    .with_context(|| format!("write {}", key_path.display()))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(&key_path)?.permissions();
                    perms.set_mode(0o600);
                    std::fs::set_permissions(&key_path, perms)?;
                }
                encoded
            }
        }
    };

    Ok((data_dir, db_url, master_key))
}

/// Construct a `Config` for dev: layer the chosen database URL and
/// master key into the struct so the existing `load_master_key` and
/// `bootstrap_persistence` see them.
fn build_dev_config(base: Arc<Config>, db_url: &str, master_key: &str) -> Arc<Config> {
    let mut modules = base.modules.clone();
    let db_value = modules
        .entry("database".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(obj) = db_value.as_object_mut() {
        obj.insert(
            "master_key".to_string(),
            serde_json::Value::String(master_key.to_string()),
        );
    }
    Arc::new(openpanel_core::Config {
        server: base.server.clone(),
        database: openpanel_core::config::DatabaseConfig {
            driver: base.database.driver.clone(),
            url: db_url.to_string(),
            max_connections: base.database.max_connections,
        },
        log: base.log.clone(),
        monitoring: base.monitoring.clone(),
        modules,
    })
}

/// Create a default `admin` / `admin` owner if the users table is empty.
/// Prints a clear warning that the password must be changed.
async fn bootstrap_default_owner(config: &Arc<Config>) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config.clone()).await?;
    let users = svc.list_users().await.context("list users")?;
    if !users.is_empty() {
        return Ok(());
    }
    let email_str = "admin@openpanel.local";
    let pw = "openpanel-dev";
    svc.create_user(
        "admin",
        email_str,
        pw,
        openpanel_domain::Role::Owner,
        "dev-bootstrap",
    )
    .await
    .context("create default owner")?;
    println!("  bootstrapped owner 'admin' (password: openpanel-dev)");
    Ok(())
}

/// `openpanel monitoring overview` — print the current host snapshot.
pub async fn monitoring_overview(config: Arc<Config>) -> anyhow::Result<()> {
    let svc = build_monitoring(config).await?;
    let snap = svc
        .snapshot_now()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "timestamp={}  cpu={:.1}%  memory={:.1}%  load={:.2}",
        snap.timestamp.to_rfc3339(),
        snap.cpu,
        snap.memory,
        snap.load
    );
    for d in &snap.disk {
        println!("disk  mount={:<20} {}%", d.mount, d.percent);
    }
    for n in &snap.network {
        println!(
            "net   iface={:<16} rx={} B/s  tx={} B/s",
            n.interface, n.rx_bytes_per_sec, n.tx_bytes_per_sec
        );
    }
    Ok(())
}

/// `openpanel monitoring history --metric <kind> [--range <secs>]`.
pub async fn monitoring_history(
    config: Arc<Config>,
    metric: String,
    range: i64,
) -> anyhow::Result<()> {
    let kind = metric
        .parse::<openpanel_domain::monitoring::MetricKind>()
        .map_err(|e: openpanel_domain::monitoring::MonitoringError| {
            anyhow::anyhow!("unknown metric `{metric}`: {e}")
        })?;
    let svc = build_monitoring(config).await?;
    let since = chrono::Utc::now() - chrono::Duration::seconds(range);
    let samples = svc
        .history(kind, since)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if samples.is_empty() {
        println!("no samples for {metric} in the last {range}s");
        return Ok(());
    }
    for s in samples {
        println!("{}  {} {metric}", s.ts.to_rfc3339(), s.value);
    }
    Ok(())
}
