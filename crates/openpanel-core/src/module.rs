//! Module registry: every bounded context implements `Module` and registers
//! itself in the composition root. Adding a domain = one `register()` call.

use std::sync::Arc;

use axum::Router;
use serde_json::Value;

use crate::audit::AuditService;
use crate::config::Config;
use crate::database::DatabaseDriver;
use crate::jobs::BackgroundTask;
use crate::migration::Migration;

/// Identifier of a registered module, e.g. `identity`.
pub type ModuleName = &'static str;

/// One HTTP router returned by a module, mounted under `/api/v1/{module_name}`.
#[derive(Debug, Clone)]
pub struct RouteMount {
    /// Path under the module's namespace, e.g. `/login` becomes
    /// `/api/v1/identity/login`.
    pub sub_path: String,
    pub router: Router,
}

pub trait Module: Send + Sync + 'static {
    fn name(&self) -> ModuleName;

    /// JSON Schema fragment for the module's own config section. Keys are
    /// nested under `{module_name}` in the final config document.
    fn config_schema(&self) -> Value {
        serde_json::json!({})
    }

    /// SQL migrations owned by this module. Filenames are scanned from
    /// `<crate>/src/migrations/<module_name>/V###__*.sql`.
    fn migrations(&self) -> Vec<Migration> {
        Vec::new()
    }

    /// HTTP routes to mount under `/api/v1/{module_name}{sub_path}`.
    fn routes(&self) -> Vec<RouteMount> {
        Vec::new()
    }

    /// Background tasks spawned by the agent supervisor.
    fn background_tasks(
        &self,
        _ctx: &AppContext,
    ) -> Vec<Box<dyn BackgroundTask>> {
        Vec::new()
    }
}

/// Shared application context handed to every module at construction time.
/// Each module may stash its own service handle in `services` if it wants
/// to share state with other modules.
#[derive(Clone)]
pub struct AppContext {
    pub config: Arc<Config>,
    pub db: Arc<dyn DatabaseDriver>,
    pub audit: Arc<dyn AuditService>,
}

impl AppContext {
    pub fn new(
        config: Arc<Config>,
        db: Arc<dyn DatabaseDriver>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self { config, db, audit }
    }
}

#[derive(Default)]
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a module. Panics on duplicate names — that is a programming
    /// error caught at startup, not a runtime concern.
    pub fn register<M: Module>(&mut self, module: M) -> &mut Self {
        let name = module.name();
        if self.modules.iter().any(|m| m.name() == name) {
            panic!("module `{name}` registered twice");
        }
        self.modules.push(Box::new(module));
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn Module> {
        self.modules.iter().map(|m| m.as_ref())
    }

    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.modules.iter().map(|m| m.name()).collect()
    }
}