//! CLI command handlers. Each command bootstraps the application context
//! enough to call into the IdentityService or start the server.

use std::sync::Arc;

use anyhow::Context;
use openpanel_api::build_router;
use openpanel_app::IdentityModule;
use openpanel_core::{
    AppContext, Config, MigrationRunner, Module, SqliteAuditService, SqliteDriver,
};
use openpanel_domain::Role;
use tokio::net::TcpListener;

pub async fn serve(config: Arc<Config>) -> anyhow::Result<()> {
    let driver = SqliteDriver::new(config.database().url.clone());
    let pool = driver.connect().await.context("connect sqlite")?;

    SqliteAuditService::new(pool.clone())
        .ensure_schema()
        .await
        .context("ensure audit schema")?;

    let audit = Arc::new(SqliteAuditService::new(pool.clone()));
    let db: Arc<dyn openpanel_core::DatabaseDriver> = Arc::new(driver);
    let ctx = AppContext::new(config.clone(), db, audit);

    // Apply migrations for each module.
    let runner = MigrationRunner::for_sqlite(pool.clone());
    let identity_module = IdentityModule::new(&ctx).await;
    runner
        .apply_module(identity_module.name(), &identity_module.migrations())
        .await
        .context("apply identity migrations")?;
    // The audit table is owned by the architecture; ensure it exists even
    // before any module records an event.
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .ok();

    let svc = identity_module.service();
    let app = build_router(svc);

    let addr = format!("{}:{}", config.server().bind, config.server().port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    tracing::info!(%addr, "openpanel listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

pub async fn migrate(config: Arc<Config>) -> anyhow::Result<()> {
    let driver = SqliteDriver::new(config.database().url.clone());
    let pool = driver.connect().await?;
    SqliteAuditService::new(pool.clone()).ensure_schema().await?;
    let audit = Arc::new(SqliteAuditService::new(pool.clone()));
    let db: Arc<dyn openpanel_core::DatabaseDriver> = Arc::new(driver);
    let ctx = AppContext::new(config.clone(), db, audit);

    let runner = MigrationRunner::for_sqlite(pool.clone());
    let identity_module = IdentityModule::new(&ctx).await;
    runner
        .apply_module(identity_module.name(), &identity_module.migrations())
        .await?;
    println!("migrations applied");
    Ok(())
}

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

pub async fn disable_user(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    svc.disable_user(uuid, "cli").await?;
    println!("disabled {id}");
    Ok(())
}

pub async fn delete_user(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    svc.delete_user(uuid, "cli").await?;
    println!("deleted {id}");
    Ok(())
}

async fn build_identity(
    config: Arc<Config>,
) -> anyhow::Result<(Arc<openpanel_app::IdentityService>, Arc<SqliteAuditService>, sqlx::Pool<sqlx::Sqlite>)> {
    let driver = SqliteDriver::new(config.database().url.clone());
    let pool = driver.connect().await?;
    SqliteAuditService::new(pool.clone()).ensure_schema().await?;
    let audit = Arc::new(SqliteAuditService::new(pool.clone()));
    let db: Arc<dyn openpanel_core::DatabaseDriver> = Arc::new(driver);
    let ctx = AppContext::new(config, db, audit.clone());
    let module = IdentityModule::new(&ctx).await;
    Ok((module.service(), audit, pool))
}