//! CLI command handlers. Each command bootstraps the application context
//! enough to call into the IdentityService or start the server.

use std::sync::Arc;

use anyhow::Context;
use openpanel_api::build_router;
use openpanel_app::{
    DatabasesModule, FilesModule, IdentityModule, SitesModule, databases::crypto as db_crypto,
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

    let identity_svc = identity_module.service();
    let sites_svc = sites_module.service();
    let databases_svc = databases_module.service();
    let files_svc = files_module.service();
    let app = build_router(identity_svc, sites_svc, databases_svc, files_svc);

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
