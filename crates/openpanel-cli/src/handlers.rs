//! CLI command handlers. Each command bootstraps the application context
//! enough to call into the IdentityService or start the server.

use std::sync::Arc;

use anyhow::Context;
use openpanel_api::build_router;
use openpanel_app::{
    AcmeEndpoint, DatabasesModule, FilesModule, IdentityModule, MonitoringModule, SitesModule,
    SslModule, SslPaths, databases::crypto as db_crypto,
};
use openpanel_core::{
    AppContext, Config, MigrationRunner, Module, SqliteAuditService, SqliteDriver,
};
use openpanel_domain::Role;
use tokio::net::TcpListener;

/// Bootstraps persistence, runs all module migrations, and serves the OpenPanel
/// API + agent over HTTP on the configured bind address until the process exits.
pub async fn serve(config: Arc<Config>) -> anyhow::Result<()> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;

    let ctx = AppContext::new(config.clone(), db, audit);

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

    let identity_svc = identity_module.service();
    let sites_svc = sites_module.service();
    let databases_svc = databases_module.service();
    let files_svc = files_module.service();
    let ssl_svc = ssl_module.service();
    let monitoring_svc = monitoring_module.service();
    let app = build_router(
        identity_svc.clone(),
        sites_svc.clone(),
        databases_svc.clone(),
        files_svc.clone(),
        ssl_svc.clone(),
        monitoring_svc.clone(),
    )
    .merge(openpanel_web::router(
        identity_svc,
        sites_svc,
        databases_svc,
        files_svc,
        ssl_svc,
        monitoring_svc,
    ));

    let addr = format!("{}:{}", config.server().bind, config.server().port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    tracing::info!(%addr, "openpanel listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
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
        .unwrap_or_else(|_| "[email protected]".to_string())
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
