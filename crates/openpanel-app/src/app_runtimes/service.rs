//! Non-PHP runtime services: supervisor unit builder, reverse
//! proxy layer, and runtime service.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, RuntimeError, RuntimeRepository, RuntimeStatus, SiteRuntime, User,
    render_nginx_proxy_block, render_supervisor_unit,
};
use uuid::Uuid;

use crate::app_runtimes::SqliteRuntimeRepository;

/// Forward the domain renderers for app callers.
pub fn render_unit(runtime: &SiteRuntime, site_user: &str, home: &str) -> String {
    render_supervisor_unit(runtime, site_user, home)
}

/// Forward the nginx proxy block render.
pub fn render_proxy_block(runtime: &SiteRuntime, server_name: &str) -> String {
    render_nginx_proxy_block(runtime, server_name)
}

/// Supervisor unit builder. The builder is a pure wrapper around
/// `render_supervisor_unit` so the production supervisor can
/// consume the same artifact as the tests.
pub struct SupervisorUnitBuilder;

impl SupervisorUnitBuilder {
    /// Construct a builder.
    pub fn new() -> Self {
        Self
    }

    /// Render the unit text.
    pub fn build(&self, runtime: &SiteRuntime, site_user: &str, home: &str) -> String {
        render_unit(runtime, site_user, home)
    }
}

impl Default for SupervisorUnitBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Reverse proxy layer. Renders an nginx `proxy_pass` block
/// that only targets loopback.
pub struct ReverseProxyLayer;

impl ReverseProxyLayer {
    /// Construct a layer.
    pub fn new() -> Self {
        Self
    }

    /// Render the proxy block.
    pub fn render(&self, runtime: &SiteRuntime, server_name: &str) -> String {
        render_proxy_block(runtime, server_name)
    }
}

impl Default for ReverseProxyLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Top-level runtime service.
pub struct RuntimeService {
    repo: Arc<SqliteRuntimeRepository>,
    audit: Arc<dyn AuditService>,
    unit: SupervisorUnitBuilder,
    proxy: ReverseProxyLayer,
}

impl RuntimeService {
    /// Construct the service.
    pub fn new(
        repo: Arc<SqliteRuntimeRepository>,
        audit: Arc<dyn AuditService>,
        unit: SupervisorUnitBuilder,
        proxy: ReverseProxyLayer,
    ) -> Self {
        Self {
            repo,
            audit,
            unit,
            proxy,
        }
    }

    /// Set (or replace) the runtime for a site.
    pub async fn set(
        &self,
        caller: &User,
        runtime: SiteRuntime,
    ) -> Result<SiteRuntime, RuntimeError> {
        require_admin(caller)?;
        runtime.validate()?;
        self.repo.save_runtime(&runtime).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::RuntimeChanged,
                    AuditOutcome::Success,
                )
                .target(runtime.site_id.to_string())
                .metadata(serde_json::json!({
                    "kind": runtime.kind.as_str(),
                    "version": runtime.version,
                })),
            )
            .await;
        Ok(runtime)
    }

    /// Read the runtime for one site.
    pub async fn get(&self, site_id: Uuid) -> Result<Option<SiteRuntime>, RuntimeError> {
        Ok(self.repo.get_runtime(site_id).await?)
    }

    /// List all runtimes.
    pub async fn list(&self) -> Result<Vec<SiteRuntime>, RuntimeError> {
        Ok(self.repo.list_runtimes().await?)
    }

    /// Update the runtime status (start / stop / restart).
    pub async fn set_status(
        &self,
        caller: &User,
        site_id: Uuid,
        status: RuntimeStatus,
    ) -> Result<SiteRuntime, RuntimeError> {
        require_admin(caller)?;
        let mut runtime = self
            .repo
            .get_runtime(site_id)
            .await?
            .ok_or(RuntimeError::Forbidden)?;
        runtime.status = status;
        self.repo.save_runtime(&runtime).await?;
        let outcome = match status {
            RuntimeStatus::Running => AuditOutcome::Success,
            RuntimeStatus::Stopped => AuditOutcome::Success,
            RuntimeStatus::Failed => AuditOutcome::Failure,
        };
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::RuntimeChanged,
                    outcome,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({ "status": status.as_str() })),
            )
            .await;
        Ok(runtime)
    }

    /// Render the supervisor unit for a runtime.
    pub fn render_unit(&self, runtime: &SiteRuntime, site_user: &str, home: &str) -> String {
        self.unit.build(runtime, site_user, home)
    }

    /// Render the nginx proxy block for a runtime.
    pub fn render_proxy(&self, runtime: &SiteRuntime, server_name: &str) -> String {
        self.proxy.render(runtime, server_name)
    }
}

fn require_admin(caller: &User) -> Result<(), RuntimeError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(RuntimeError::Forbidden),
    }
}
