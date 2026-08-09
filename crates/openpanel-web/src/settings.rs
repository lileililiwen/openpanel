//! Owner-only panel preferences and redacted installation information.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Form,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService, Config};
use openpanel_domain::Role;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// Safe, mutable browser preferences. This deliberately excludes operational
/// and secret-bearing configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelPreferences {
    /// Color theme (`dark`, `light`, or `system`).
    pub theme: String,
    /// UI locale supported by the embedded translations.
    pub locale: String,
    /// IANA timezone used for displaying timestamps.
    pub timezone: String,
}

impl Default for PanelPreferences {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            locale: "en-US".to_string(),
            timezone: "UTC".to_string(),
        }
    }
}

impl PanelPreferences {
    fn validate(&self) -> Result<(), SettingsError> {
        if !matches!(self.theme.as_str(), "dark" | "light" | "system") {
            return Err(SettingsError::Validation("unsupported theme".to_string()));
        }
        if !matches!(self.locale.as_str(), "en-US" | "zh-CN") {
            return Err(SettingsError::Validation("unsupported locale".to_string()));
        }
        validate_timezone(&self.timezone)?;
        Ok(())
    }

    /// Load a persisted preference file, falling back to safe defaults when
    /// the file is absent or invalid.
    pub fn load_or_default(path: &Path) -> Self {
        let loaded = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|preferences| preferences.validate().is_ok());
        loaded.unwrap_or_default()
    }
}

/// Partial update accepted by the preference service.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsUpdate {
    /// Replacement theme when present.
    pub theme: Option<String>,
    /// Replacement locale when present.
    pub locale: Option<String>,
    /// Replacement timezone when present.
    pub timezone: Option<String>,
}

/// Redacted installation details safe to render to an Owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationInfo {
    /// Running OpenPanel version.
    pub version: &'static str,
    /// Application data root.
    pub data_path: String,
    /// Effective operator configuration path.
    pub config_path: String,
    /// Configured database driver, never its connection URL.
    pub database_driver: String,
    /// HTTP bind address and port.
    pub listen_address: String,
}

impl InstallationInfo {
    /// Build a metadata-only view. Secret-bearing configuration values are not
    /// copied into this type, making accidental template leakage impossible.
    pub fn from_config(config: &Config, data_path: &str, config_path: &str) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            data_path: data_path.to_string(),
            config_path: config_path.to_string(),
            database_driver: config.database.driver.clone(),
            listen_address: format!("{}:{}", config.server.bind, config.server.port),
        }
    }
}

/// Preference persistence or validation failure.
#[derive(Debug, Error)]
pub enum SettingsError {
    /// An allowlisted value failed validation.
    #[error("{0}")]
    Validation(String),
    /// Atomic persistence failed.
    #[error("could not persist panel preferences: {0}")]
    Persistence(String),
    /// The required audit event could not be written.
    #[error("could not audit panel preferences: {0}")]
    Audit(String),
}

/// Concurrent preference state backed by an atomically replaced JSON file.
pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<PanelPreferences>,
    audit: Arc<dyn AuditService>,
}

impl SettingsStore {
    /// Create a store initialized from validated effective preferences.
    pub fn new(path: PathBuf, initial: PanelPreferences, audit: Arc<dyn AuditService>) -> Self {
        Self {
            path,
            current: RwLock::new(initial),
            audit,
        }
    }

    /// Read the current preferences.
    pub async fn current(&self) -> PanelPreferences {
        self.current.read().await.clone()
    }

    /// Validate, atomically persist, audit field names, and publish an update.
    pub async fn update(
        &self,
        actor: &str,
        update: SettingsUpdate,
    ) -> Result<PanelPreferences, SettingsError> {
        let mut guard = self.current.write().await;
        let mut next = guard.clone();
        let mut fields = Vec::new();
        if let Some(theme) = update.theme {
            next.theme = theme;
            fields.push("theme");
        }
        if let Some(locale) = update.locale {
            next.locale = locale;
            fields.push("locale");
        }
        if let Some(timezone) = update.timezone {
            next.timezone = timezone;
            fields.push("timezone");
        }
        if fields.is_empty() {
            return Err(SettingsError::Validation(
                "no supported fields supplied".to_string(),
            ));
        }
        next.validate()?;
        persist_atomically(&self.path, &next).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::SettingsChanged, AuditOutcome::Success)
                    .target("panel_preferences")
                    .metadata(serde_json::json!({ "fields": fields })),
            )
            .await
            .map_err(|error| SettingsError::Audit(error.to_string()))?;
        *guard = next.clone();
        Ok(next)
    }

    /// On-disk preference path, used by installation diagnostics and tests.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

async fn persist_atomically(
    path: &Path,
    preferences: &PanelPreferences,
) -> Result<(), SettingsError> {
    let parent = path
        .parent()
        .ok_or_else(|| SettingsError::Persistence("preference path has no parent".to_string()))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| SettingsError::Persistence(error.to_string()))?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(preferences)
        .map_err(|error| SettingsError::Persistence(error.to_string()))?;
    let result = async {
        tokio::fs::write(&temporary, bytes).await?;
        let file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&temporary)
            .await?;
        file.sync_all().await?;
        tokio::fs::rename(&temporary, path).await
    }
    .await;
    if let Err(error) = result {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(SettingsError::Persistence(error.to_string()));
    }
    Ok(())
}

fn validate_timezone(timezone: &str) -> Result<(), SettingsError> {
    let safe = !timezone.is_empty()
        && !timezone.starts_with('/')
        && timezone.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
        });
    if !safe || !Path::new("/usr/share/zoneinfo").join(timezone).is_file() {
        return Err(SettingsError::Validation(
            "unknown IANA timezone".to_string(),
        ));
    }
    Ok(())
}

/// Strict HTML form; unknown fields are rejected by the form extractor.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsForm {
    theme: String,
    locale: String,
    timezone: String,
    #[serde(default)]
    _csrf: String,
}

/// GET `/settings`.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    if !matches!(user.role(), Role::Owner) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let preferences = state.settings.current().await;
    let content = render_settings(&state.installation, &preferences, &csrf, None);
    state
        .render_shell(&user, &csrf, "/settings", content)
        .await
        .into_response()
}

/// POST `/settings`.
pub async fn update(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<SettingsForm>,
) -> Response {
    if !matches!(user.role(), Role::Owner) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let result = state
        .settings
        .update(
            user.username().as_str(),
            SettingsUpdate {
                theme: Some(form.theme),
                locale: Some(form.locale),
                timezone: Some(form.timezone),
            },
        )
        .await;
    let (status, preferences, message): (StatusCode, PanelPreferences, Option<String>) =
        match result {
            Ok(preferences) => (
                StatusCode::OK,
                preferences,
                Some("Settings saved".to_string()),
            ),
            Err(SettingsError::Validation(message)) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                state.settings.current().await,
                Some(message),
            ),
            Err(error) => {
                tracing::error!(%error, "settings update failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    state.settings.current().await,
                    Some("Settings could not be saved".to_string()),
                )
            }
        };
    let content = render_settings(&state.installation, &preferences, &csrf, message.as_deref());
    (
        status,
        state.render_shell(&user, &csrf, "/settings", content).await,
    )
        .into_response()
}

/// Render the settings content region with metadata-only installation data.
pub fn render_settings(
    info: &InstallationInfo,
    preferences: &PanelPreferences,
    csrf: &str,
    message: Option<&str>,
) -> Markup {
    html! {
        h1 { "Settings" }
        @if let Some(message) = message { p class="alert" { (message) } }
        section class="detail installation-info" {
            h2 { "Installation" }
            dl {
                dt { "Version" } dd { (info.version) }
                dt { "Data path" } dd { (info.data_path) }
                dt { "Config path" } dd { (info.config_path) }
                dt { "Database driver" } dd { (info.database_driver) }
                dt { "Listen address" } dd { (info.listen_address) }
            }
        }
        form class="form" method="post" action="/settings" {
            (csrf_field(csrf))
            label for="theme" { "Theme" }
            select id="theme" name="theme" {
                @for value in ["dark", "light", "system"] {
                    option value=(value) selected[preferences.theme == value] { (value) }
                }
            }
            label for="locale" { "Locale" }
            select id="locale" name="locale" {
                @for value in ["en-US", "zh-CN"] {
                    option value=(value) selected[preferences.locale == value] { (value) }
                }
            }
            label for="timezone" { "Timezone" }
            input id="timezone" name="timezone" type="text" value=(preferences.timezone);
            button type="submit" { "Save settings" }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use openpanel_core::{Config, NoopAuditService};

    use super::*;

    #[test]
    fn installation_info_redacts_secret_configuration() {
        let mut config = Config::default();
        config.database.url = "sqlite://admin:database-secret@localhost/openpanel".to_string();
        config.modules.insert(
            "ssl".to_string(),
            serde_json::json!({
                "master_key": "master-key-secret",
                "session_token": "session-token-secret",
                "private_key": "private-key-secret"
            }),
        );

        let info = InstallationInfo::from_config(
            &config,
            "/var/lib/openpanel",
            "/etc/openpanel/openpanel.toml",
        );
        let body =
            render_settings(&info, &PanelPreferences::default(), "csrf-token", None).into_string();

        assert!(body.contains(env!("CARGO_PKG_VERSION")));
        assert!(body.contains("/var/lib/openpanel"));
        assert!(body.contains("/etc/openpanel/openpanel.toml"));
        for secret in [
            "database-secret",
            "master-key-secret",
            "session-token-secret",
            "private-key-secret",
        ] {
            assert!(!body.contains(secret), "leaked {secret}: {body}");
        }
    }

    #[tokio::test]
    async fn settings_store_persists_allowlisted_preferences_atomically() {
        let dir = tempfile::tempdir().expect("settings tempdir");
        let path = dir.path().join("preferences.json");
        let store = SettingsStore::new(
            path.clone(),
            PanelPreferences::default(),
            Arc::new(NoopAuditService),
        );

        let updated = store
            .update(
                "owner",
                SettingsUpdate {
                    theme: Some("light".to_string()),
                    locale: Some("en-US".to_string()),
                    timezone: Some("Asia/Shanghai".to_string()),
                },
            )
            .await
            .expect("valid settings update");

        assert_eq!(updated.theme, "light");
        assert_eq!(updated.locale, "en-US");
        assert_eq!(updated.timezone, "Asia/Shanghai");
        let persisted = std::fs::read_to_string(path).expect("preferences persisted");
        assert!(persisted.contains("Asia/Shanghai"));
        assert!(!dir.path().join("preferences.json.tmp").exists());
    }

    #[tokio::test]
    async fn settings_store_rejects_invalid_values_without_changing_state() {
        let dir = tempfile::tempdir().expect("settings tempdir");
        let store = SettingsStore::new(
            dir.path().join("preferences.json"),
            PanelPreferences::default(),
            Arc::new(NoopAuditService),
        );

        let result = store
            .update(
                "owner",
                SettingsUpdate {
                    theme: Some("neon".to_string()),
                    locale: None,
                    timezone: Some("Not/A_Real_Zone".to_string()),
                },
            )
            .await;

        assert!(result.is_err());
        assert_eq!(store.current().await, PanelPreferences::default());
    }
}
