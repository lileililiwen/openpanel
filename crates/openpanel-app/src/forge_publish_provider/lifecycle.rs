//! Process lifecycle harness for the optional Forge publish
//! provider. The provider is started and stopped independently of
//! the main OpenPanel service: when the provider is absent,
//! disabled, or has crashed, the main service is unaffected. The
//! harness models that guarantee as an explicit, testable state
//! machine instead of relying on process supervision side effects.

use std::env;

use serde::{Deserialize, Serialize};

/// Environment variable Forge (or the operator) sets to enable the
/// provider. Absent or any value other than a truthy literal means
/// "disabled", which is the default: OpenPanel never turns the
/// provider on by itself.
pub const FORGE_PROVIDER_ENABLE_ENV: &str = "OPENPANEL_FORGE_PUBLISH_PROVIDER";

/// Lifecycle state of the optional provider process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeProviderProcessStatus {
    /// The provider is not enabled; no process is expected.
    Disabled,
    /// The provider is enabled but not currently running.
    Stopped,
    /// The provider is enabled and running.
    Running,
    /// The provider exited unexpectedly; the main service is still
    /// healthy.
    Failed,
}

impl ForgeProviderProcessStatus {
    /// Whether the main service may continue serving traffic in
    /// this provider state. Always `true`: the provider is optional.
    pub fn main_service_operational(self) -> bool {
        true
    }
}

/// In-process model of the provider process lifecycle. The harness
/// owns no OS process in v0.1; the JSON-RPC adapter in `protocol.rs`
/// is driven by whoever owns the transport (Forge, a systemd unit, a
/// test). The harness captures the start/stop contract so the
/// "provider is optional" guarantee is provable.
#[derive(Debug, Clone)]
pub struct ForgeProviderProcessHandle {
    enabled: bool,
    status: ForgeProviderProcessStatus,
}

impl ForgeProviderProcessHandle {
    /// Build a handle for an explicitly enabled/disabled provider.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            status: if enabled {
                ForgeProviderProcessStatus::Stopped
            } else {
                ForgeProviderProcessStatus::Disabled
            },
        }
    }

    /// Build a handle from the process environment.
    pub fn from_environment() -> Self {
        let enabled = env::var(FORGE_PROVIDER_ENABLE_ENV)
            .ok()
            .as_deref()
            .is_some_and(parse_provider_enabled);
        Self::new(enabled)
    }

    /// Whether the provider is enabled.
    pub fn is_provider_enabled(&self) -> bool {
        self.enabled
    }

    /// Current lifecycle status.
    pub fn provider_status(&self) -> ForgeProviderProcessStatus {
        self.status
    }

    /// Start the provider. A disabled provider stays `Disabled`;
    /// an enabled provider moves to `Running`.
    pub fn start_provider(&mut self) -> ForgeProviderProcessStatus {
        if self.enabled {
            self.status = ForgeProviderProcessStatus::Running;
        }
        self.status
    }

    /// Stop the provider. A disabled provider stays `Disabled`; an
    /// enabled provider moves to `Stopped`.
    pub fn stop_provider(&mut self) -> ForgeProviderProcessStatus {
        if self.enabled {
            self.status = ForgeProviderProcessStatus::Stopped;
        }
        self.status
    }

    /// Record an unexpected provider failure. The main service is
    /// still operational.
    pub fn mark_provider_failed(&mut self) -> ForgeProviderProcessStatus {
        if self.enabled {
            self.status = ForgeProviderProcessStatus::Failed;
        }
        self.status
    }
}

impl Default for ForgeProviderProcessHandle {
    fn default() -> Self {
        Self::from_environment()
    }
}

/// Parse a truthy literal. `1`, `true`, `yes`, and `on` (any case)
/// enable the provider; everything else keeps it disabled.
pub fn parse_provider_enabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}
