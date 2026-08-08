//! Module registry: every bounded context implements `Module` and registers
//! itself in the composition root. Adding a domain = one `register()` call.

use std::sync::Arc;

use axum::Router;
use serde_json::Value;

use crate::{
    audit::AuditService, config::Config, database::DatabaseDriver, error::CoreError,
    jobs::BackgroundTask, migration::Migration,
};

/// Identifier of a registered module, e.g. `identity`.
pub type ModuleName = &'static str;

/// One HTTP router returned by a module, mounted under `/api/v1/{module_name}`.
#[derive(Debug, Clone)]
pub struct RouteMount {
    /// Path under the module's namespace, e.g. `/login` becomes
    /// `/api/v1/identity/login`.
    pub sub_path: String,
    /// The axum router to mount at the composed path.
    pub router: Router,
}

/// Contract every bounded-context module must implement to register itself
/// in the composition root.
pub trait Module: Send + Sync + 'static {
    /// Unique name of the module, used as its config key and URL namespace.
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
    fn background_tasks(&self, _ctx: &AppContext) -> Vec<Box<dyn BackgroundTask>> {
        Vec::new()
    }
}

/// Shared application context handed to every module at construction time.
/// Each module may stash its own service handle in `services` if it wants
/// to share state with other modules.
#[derive(Clone)]
pub struct AppContext {
    /// Shared validated configuration.
    pub config: Arc<Config>,
    /// Shared database driver.
    pub db: Arc<dyn DatabaseDriver>,
    /// Shared audit service.
    pub audit: Arc<dyn AuditService>,
}

impl AppContext {
    /// Create a new application context from shared services.
    pub fn new(
        config: Arc<Config>,
        db: Arc<dyn DatabaseDriver>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self { config, db, audit }
    }
}

/// Registry of registered modules, enforcing unique names.
#[derive(Default)]
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl ModuleRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a module. Returns `CoreError::DuplicateModule` on duplicate
    /// names — a programming error caught at startup, not a runtime concern.
    pub fn register<M: Module>(&mut self, module: M) -> Result<&mut Self, CoreError> {
        let name = module.name();
        if self.modules.iter().any(|m| m.name() == name) {
            return Err(CoreError::DuplicateModule(name.to_string()));
        }
        self.modules.push(Box::new(module));
        Ok(self)
    }

    /// Iterate over all registered modules.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Module> {
        self.modules.iter().map(|m| m.as_ref())
    }

    /// Number of registered modules.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether no modules are registered.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Names of all registered modules.
    pub fn names(&self) -> Vec<&'static str> {
        self.modules.iter().map(|m| m.name()).collect()
    }
}
