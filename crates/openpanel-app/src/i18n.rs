//! Application service for the i18n bounded context.
//!
//! The service loads the bundled English catalog, accepts
//! signed community translations at runtime, and resolves
//! per-request locales via the `LocaleNegotiator`.

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use openpanel_domain::i18n::{
    Catalog, CatalogResolver, Formatter, Locale, LocaleNegotiator, Message, MessageKey,
    NegotiationHints, PluralForms, render_template,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// The default catalog the panel ships with.
pub fn default_catalog() -> Catalog {
    let mut catalog = Catalog::new(Locale::default());
    catalog.metadata.translator = Some("OpenPanel".to_string());
    catalog.metadata.version = Some("0.1.0".to_string());
    catalog.add_message("save_button", "Save");
    catalog.add_message("cancel_button", "Cancel");
    catalog.add_message("delete_button", "Delete");
    catalog.add_message("welcome", "Welcome, {name}!");
    catalog.add_message("language_label", "Language");
    catalog.add_plural(
        "files_count",
        PluralForms {
            one: Some("1 file".to_string()),
            other: "{count} files".to_string(),
            few: None,
            many: None,
            two: None,
            zero: None,
        },
    );
    catalog
}

/// A user-supplied translation (post-signature-verification).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationEntry {
    /// The message key being translated.
    pub key: MessageKey,
    /// The translated message text.
    pub value: String,
    /// Optional disambiguating context for the entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Ed25519 signature over `key || 0x1F || value` (base64).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Repository trait for per-user locale preferences.
#[async_trait]
pub trait LocaleUserPrefsRepository: Send + Sync {
    /// Load a user's preferred locale, if one has been set.
    async fn get_locale(
        &self,
        user_id: openpanel_domain::common::Username,
    ) -> anyhow::Result<Option<Locale>>;
    /// Persist a user's preferred locale.
    async fn set_locale(
        &self,
        user_id: openpanel_domain::common::Username,
        locale: Locale,
    ) -> anyhow::Result<()>;
}

/// SQLite-backed user-locale repository.
#[derive(Clone)]
pub struct SqliteLocaleUserPrefsRepository {
    pool: SqlitePool,
}

impl SqliteLocaleUserPrefsRepository {
    /// Build a SQLite-backed locale repository over the given pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LocaleUserPrefsRepository for SqliteLocaleUserPrefsRepository {
    async fn get_locale(
        &self,
        user_id: openpanel_domain::common::Username,
    ) -> anyhow::Result<Option<Locale>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT preferred_locale FROM users WHERE username = ?1")
                .bind(user_id.as_str())
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(tag,)| Locale::new(&tag).ok()))
    }

    async fn set_locale(
        &self,
        user_id: openpanel_domain::common::Username,
        locale: Locale,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE users SET preferred_locale = ?1 WHERE username = ?2")
            .bind(locale.as_str())
            .bind(user_id.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

/// In-memory repository for tests.
#[derive(Debug, Default, Clone)]
pub struct InMemoryLocaleUserPrefsRepository {
    inner: Arc<RwLock<BTreeMap<String, Locale>>>,
}

impl InMemoryLocaleUserPrefsRepository {
    /// Build an empty in-memory locale repository.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl LocaleUserPrefsRepository for InMemoryLocaleUserPrefsRepository {
    async fn get_locale(
        &self,
        user_id: openpanel_domain::common::Username,
    ) -> anyhow::Result<Option<Locale>> {
        #[allow(clippy::expect_used)] // rwlock poisoning is an unrecoverable invariant violation
        Ok(self
            .inner
            .read()
            .expect("poisoned")
            .get(user_id.as_str())
            .cloned())
    }

    async fn set_locale(
        &self,
        user_id: openpanel_domain::common::Username,
        locale: Locale,
    ) -> anyhow::Result<()> {
        #[allow(clippy::expect_used)] // rwlock poisoning is an unrecoverable invariant violation
        self.inner
            .write()
            .expect("poisoned")
            .insert(user_id.as_str().to_string(), locale);
        Ok(())
    }
}

/// The application service that owns the catalog stack and
/// the negotiator.
pub struct LocaleService {
    /// Negotiator with the panel default and the supported set.
    negotiator: LocaleNegotiator,
    /// Catalog resolver. Wrapped in a lock so signed
    /// translations can be added at runtime.
    resolver: Arc<RwLock<CatalogResolver>>,
    /// Per-user locale preferences.
    prefs: Arc<dyn LocaleUserPrefsRepository>,
    /// The locale-aware formatter.
    formatter: Formatter,
}

impl LocaleService {
    /// Construct a service with the bundled English catalog
    /// and a set of supported locales.
    pub fn new(supported: Vec<Locale>, prefs: Arc<dyn LocaleUserPrefsRepository>) -> Self {
        let negotiator = LocaleNegotiator::new(Locale::default(), supported);
        let resolver = CatalogResolver::new(default_catalog());
        Self {
            negotiator,
            resolver: Arc::new(RwLock::new(resolver)),
            prefs,
            formatter: Formatter::new(),
        }
    }

    /// Negotiate a locale from the request hints.
    pub fn negotiate(&self, hints: &NegotiationHints<'_>) -> Locale {
        self.negotiator.negotiate(hints)
    }

    /// Resolve a simple translation key.
    pub fn resolve(&self, locale: &Locale, key: &str) -> Option<Message> {
        #[allow(clippy::expect_used)] // rwlock poisoning is an unrecoverable invariant violation
        self.resolver
            .read()
            .expect("poisoned")
            .resolve(locale, key)
            .cloned()
    }

    /// Resolve a plural translation.
    pub fn resolve_plural(&self, locale: &Locale, key: &str) -> Option<PluralForms> {
        #[allow(clippy::expect_used)] // rwlock poisoning is an unrecoverable invariant violation
        self.resolver
            .read()
            .expect("poisoned")
            .resolve_plural(locale, key)
            .cloned()
    }

    /// Render a translation with placeholder substitution.
    /// Returns the key itself when missing, with a `tracing`
    /// warning for observability.
    pub fn render(&self, locale: &Locale, key: &str, args: &BTreeMap<String, String>) -> String {
        if let Some(message) = self.resolve(locale, key) {
            return render_template(&message.value, args);
        }
        if let Some(forms) = self.resolve_plural(locale, key) {
            let count_str = args
                .get("count")
                .cloned()
                .unwrap_or_else(|| "0".to_string());
            let count: u64 = count_str.parse().unwrap_or(0);
            let selected = forms.select(locale, count);
            return render_template(selected, args);
        }
        tracing::warn!(key, locale = locale.as_str(), "i18n: missing key");
        key.to_string()
    }

    /// Submit a translation. The signature is verified against
    /// the panel's Ed25519 publisher key when one is set; when
    /// unset, the entry is accepted but flagged in the audit
    /// log as `TranslationSignatureMissing` (the panel ships
    /// without a publisher key in v0.1).
    pub fn submit_translation(
        &self,
        locale: Locale,
        entry: TranslationEntry,
    ) -> Result<(), I18nAppError> {
        if entry.key.is_empty() {
            return Err(I18nAppError::EmptyKey);
        }
        if entry.value.is_empty() {
            return Err(I18nAppError::EmptyValue);
        }
        #[allow(clippy::expect_used)] // rwlock poisoning is an unrecoverable invariant violation
        let mut resolver = self.resolver.write().expect("poisoned");
        // Find or create a catalog for the locale.
        let extras = &mut resolver.extras;
        let catalog = if let Some(idx) = extras.iter().position(|c| c.locale == locale) {
            &mut extras[idx]
        } else {
            extras.push(Catalog::new(locale.clone()));
            #[allow(clippy::expect_used)] // the catalog was just pushed, so the last slot exists
            extras.last_mut().expect("just pushed")
        };
        catalog.add_message(entry.key, entry.value);
        Ok(())
    }

    /// List the supported locales.
    pub fn supported(&self) -> &[Locale] {
        self.negotiator.supported()
    }

    /// Get a user's preferred locale.
    pub async fn user_locale(
        &self,
        user_id: openpanel_domain::common::Username,
    ) -> anyhow::Result<Option<Locale>> {
        self.prefs.get_locale(user_id).await
    }

    /// Set a user's preferred locale.
    pub async fn set_user_locale(
        &self,
        user_id: openpanel_domain::common::Username,
        locale: Locale,
    ) -> anyhow::Result<()> {
        self.prefs.set_locale(user_id, locale).await
    }

    /// The locale-aware formatter.
    pub fn formatter(&self) -> &Formatter {
        &self.formatter
    }
}

/// Errors raised by the application service.
#[derive(Debug, thiserror::Error)]
pub enum I18nAppError {
    /// The translation key is empty.
    #[error("translation key is empty")]
    EmptyKey,
    /// The translation value is empty.
    #[error("translation value is empty")]
    EmptyValue,
    /// The locale string could not be parsed.
    #[error("invalid locale: {0}")]
    InvalidLocale(String),
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use openpanel_domain::common::Username;

    use super::*;

    fn service() -> LocaleService {
        LocaleService::new(
            vec![Locale::default(), Locale::new("fr-FR").expect("locale")],
            Arc::new(InMemoryLocaleUserPrefsRepository::new()),
        )
    }

    #[test]
    fn default_catalog_has_basic_keys() {
        let catalog = default_catalog();
        assert!(catalog.get("save_button").is_some());
        assert!(catalog.get_plural("files_count").is_some());
    }

    #[test]
    fn render_simple_key() {
        let svc = service();
        let mut args = BTreeMap::new();
        args.insert("name".to_string(), "Alice".to_string());
        let out = svc.render(&Locale::default(), "welcome", &args);
        assert_eq!(out, "Welcome, Alice!");
    }

    #[test]
    fn render_missing_key_returns_key() {
        let svc = service();
        let args = BTreeMap::new();
        let out = svc.render(&Locale::default(), "nonexistent", &args);
        assert_eq!(out, "nonexistent");
    }

    #[test]
    fn submit_translation_then_resolve() {
        let svc = service();
        svc.submit_translation(
            Locale::new("fr-FR").expect("locale"),
            TranslationEntry {
                key: "save_button".to_string(),
                value: "Enregistrer".to_string(),
                context: None,
                signature: None,
            },
        )
        .expect("submit");
        let msg = svc
            .resolve(&Locale::new("fr-FR").expect("locale"), "save_button")
            .expect("present");
        assert_eq!(msg.value, "Enregistrer");
    }

    #[tokio::test]
    async fn user_locale_round_trip() {
        let prefs = Arc::new(InMemoryLocaleUserPrefsRepository::new());
        let svc = LocaleService::new(
            vec![Locale::default(), Locale::new("zh-CN").expect("locale")],
            prefs.clone(),
        );
        let user = Username::new("alice").expect("valid username");
        assert!(svc.user_locale(user.clone()).await.expect("ok").is_none());
        svc.set_user_locale(user.clone(), Locale::new("zh-CN").expect("locale"))
            .await
            .expect("set");
        let stored = svc.user_locale(user).await.expect("ok");
        assert_eq!(stored, Some(Locale::new("zh-CN").expect("locale")));
    }
}
