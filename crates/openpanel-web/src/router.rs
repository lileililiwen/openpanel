//! Web router: shell, login/logout, and static assets, all behind the same
//! session middleware used by the API so there is one auth system.

use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::LOCATION},
    middleware::{from_fn, from_fn_with_state},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::Markup;
use openpanel_api::{
    extract::AuthSession,
    middleware::session::{SESSION_COOKIE, session_middleware},
};
use openpanel_app::{
    ApiTokenService, BackupService, CollaboratorService, ContainerRegistryService,
    ContainerRuntimeService, CronService, DatabasesService, DnsService, DockerService,
    FilesService, FtpService, IdentityService, LogService, MailService, MonitoringService,
    NotificationService, PitrService, SecurityService, SitesService, SoftwareCenterService,
    SslService, StagingService, WafService, identity::TwoFactorService,
    security::LoginThrottleService, system_services::ServiceManager,
};
use openpanel_core::{AuditService, Config};
use openpanel_domain::{Session, SessionToken, User};

use crate::{
    assets, backups, cron,
    csrf::{CsrfStore, ValidateCsrf},
    dashboard, databases, files,
    layout::CapabilitySet,
    login, logs, monitoring, security, settings,
    settings::{InstallationInfo, PanelPreferences, SettingsStore},
    sites, ssl, users,
};

/// Runtime-only dependencies and installation paths used by the web adapter.
pub struct WebRuntime {
    config: Arc<Config>,
    audit: Arc<dyn AuditService>,
    preferences_path: PathBuf,
    data_path: String,
    config_path: String,
    capabilities: CapabilitySet,
}

impl WebRuntime {
    /// Create web runtime context from the composition root.
    pub fn new(
        config: Arc<Config>,
        audit: Arc<dyn AuditService>,
        preferences_path: PathBuf,
        data_path: impl Into<String>,
        config_path: impl Into<String>,
    ) -> Self {
        Self {
            config,
            audit,
            preferences_path,
            data_path: data_path.into(),
            config_path: config_path.into(),
            capabilities: CapabilitySet::shipped(),
        }
    }

    /// Override registered browser capabilities when composing future modules.
    pub fn with_capabilities(mut self, capabilities: CapabilitySet) -> Self {
        self.capabilities = capabilities;
        self
    }
}

/// Shared state for every web handler.
#[derive(Clone)]
pub struct WebState {
    /// Identity service for login/logout/session resolution.
    pub identity: Arc<IdentityService>,
    /// Sites service for the dashboard quick-count cards.
    pub sites: Arc<SitesService>,
    /// Databases service for the dashboard quick-count cards.
    pub databases: Arc<DatabasesService>,
    /// Files service for the dashboard quick-count cards.
    pub files: Arc<FilesService>,
    /// SSL service for the dashboard quick-count cards.
    pub ssl: Arc<SslService>,
    /// Monitoring service for the host gauges and alert feed.
    pub monitoring: Arc<MonitoringService>,
    /// Cron scheduling service.
    pub cron: Arc<CronService>,
    /// Backup and restore service.
    pub backups: Arc<BackupService>,
    /// Authorized log browsing service.
    pub logs: Arc<LogService>,
    /// Host firewall and login-abuse service.
    pub security: Arc<SecurityService>,
    /// Durable pre-authentication abuse protection.
    pub login_throttle: Arc<LoginThrottleService>,
    /// Allowlisted host-service manager.
    pub system_services: Arc<ServiceManager>,
    /// Provider-backed DNS service.
    pub dns: Arc<DnsService>,
    /// Hosted mail administration service.
    pub mail: Arc<MailService>,
    /// Curated Software Center service.
    pub software_center: Arc<SoftwareCenterService>,
    /// Two-factor authentication service for the settings/security page.
    pub two_factor: Arc<TwoFactorService>,
    /// Per-site web application firewall service.
    pub waf: Arc<WafService>,
    /// Least-privilege container lifecycle service.
    pub docker: Arc<DockerService>,
    /// Per-site FTP account service.
    pub ftp: Arc<FtpService>,
    /// Scoped personal API-token lifecycle.
    pub api_tokens: Arc<ApiTokenService>,
    /// Durable notification lifecycle.
    pub notifications: Arc<NotificationService>,
    /// Database point-in-time recovery service.
    pub pitr: Arc<PitrService>,
    /// Per-site staging service.
    pub staging: Arc<StagingService>,
    /// Per-site collaborator service.
    pub collaborators: Arc<CollaboratorService>,
    /// Container registry service.
    pub registry: Arc<ContainerRegistryService>,
    /// Per-user container quota, registry credentials, metrics, and egress.
    pub container_runtime: Arc<ContainerRuntimeService>,
    /// Themeable UI / white-label service.
    pub themeable_ui: Arc<openpanel_app::ThemeableUiService>,
    /// Webmail client service.
    pub webmail: Arc<openpanel_app::WebmailService>,
    /// Per-session CSRF token store.
    pub csrf: Arc<CsrfStore>,
    /// Atomically persisted allowlisted panel preferences.
    pub settings: Arc<SettingsStore>,
    /// Redacted installation metadata for the Owner settings page.
    pub installation: Arc<InstallationInfo>,
    /// Capabilities registered in this router composition.
    pub capabilities: CapabilitySet,
}

impl WebState {
    /// Render a full shell using the registered capabilities, caller role, and
    /// latest persisted display preferences.
    pub async fn render_shell(
        &self,
        user: &User,
        csrf: &str,
        path: &str,
        content: Markup,
    ) -> Markup {
        let preferences = self.settings.current().await;
        crate::layout::Shell::new(user.username().as_str(), csrf, content)
            .with_navigation(user.role(), path, self.capabilities.clone())
            .with_preferences(
                &preferences.theme,
                &preferences.locale,
                &preferences.timezone,
            )
            .render()
    }
}

/// Authenticated web user; rejects unauthenticated requests with a 302 to `/login`.
pub struct WebUser(pub User, pub Session);

impl<S> FromRequestParts<S> for WebUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let auth = parts.extensions.get::<AuthSession>().cloned();
        std::future::ready(match auth {
            Some(a) => Ok(WebUser(a.user, a.session)),
            None => Err(unauth_redirect()),
        })
    }
}

/// 302 Found → `/login`, used when an authenticated web route is hit without a
/// valid session.
fn unauth_redirect() -> Response {
    (StatusCode::FOUND, [(LOCATION, "/login")]).into_response()
}

/// POST /logout — invalidate the session, clear the cookie, redirect to login.
async fn logout(
    State(state): State<WebState>,
    WebUser(user, _session): WebUser,
    headers: HeaderMap,
    _csrf: ValidateCsrf,
) -> Response {
    if let Some(tok) = cookie_token(&headers)
        && let Ok(token) = SessionToken::from_string(tok)
    {
        let _ = state
            .identity
            .logout(&token, user.username().as_str())
            .await;
    }
    let mut resp = Redirect::to("/login").into_response();
    let cookie = format!("{SESSION_COOKIE}=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0");
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        resp.headers_mut()
            .insert(axum::http::header::SET_COOKIE, value);
    }
    resp
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

/// Build the web router. Returns a `Router<()>` ready to merge into the API app.
#[allow(clippy::too_many_arguments)]
pub fn router(
    identity: Arc<IdentityService>,
    sites: Arc<SitesService>,
    databases: Arc<DatabasesService>,
    files: Arc<FilesService>,
    ssl: Arc<SslService>,
    monitoring: Arc<MonitoringService>,
    cron: Arc<CronService>,
    backups: Arc<BackupService>,
    logs: Arc<LogService>,
    security_service: Arc<SecurityService>,
    login_throttle: Arc<LoginThrottleService>,
    system_services_service: Arc<ServiceManager>,
    dns_service: Arc<DnsService>,
    mail_service: Arc<MailService>,
    software_center_service: Arc<SoftwareCenterService>,
    two_factor: Arc<TwoFactorService>,
    waf: Arc<WafService>,
    docker: Arc<DockerService>,
    ftp: Arc<FtpService>,
    api_tokens: Arc<ApiTokenService>,
    notifications: Arc<NotificationService>,
    pitr: Arc<PitrService>,
    staging: Arc<StagingService>,
    collaborators: Arc<CollaboratorService>,
    registry: Arc<ContainerRegistryService>,
    container_runtime: Arc<ContainerRuntimeService>,
    themeable_ui: Arc<openpanel_app::ThemeableUiService>,
    webmail: Arc<openpanel_app::WebmailService>,
    runtime: WebRuntime,
) -> Router {
    let initial_preferences = PanelPreferences::load_or_default(&runtime.preferences_path);
    let installation =
        InstallationInfo::from_config(&runtime.config, &runtime.data_path, &runtime.config_path);
    let state = WebState {
        identity: identity.clone(),
        sites,
        databases,
        files,
        ssl,
        monitoring,
        cron,
        backups,
        logs,
        security: security_service,
        login_throttle,
        system_services: system_services_service,
        dns: dns_service,
        mail: mail_service,
        software_center: software_center_service,
        two_factor,
        waf,
        docker,
        ftp,
        api_tokens,
        notifications,
        pitr,
        staging,
        collaborators,
        registry,
        container_runtime,
        themeable_ui,
        webmail,
        csrf: Arc::new(CsrfStore::new()),
        settings: Arc::new(SettingsStore::new(
            runtime.preferences_path,
            initial_preferences,
            runtime.audit,
        )),
        installation: Arc::new(installation),
        capabilities: runtime.capabilities,
    };
    Router::new()
        .route("/", get(dashboard::home))
        .route("/dashboard", get(dashboard::home))
        .route("/dashboard/gauges", get(dashboard::gauges_partial))
        .route(
            "/docker",
            get(crate::docker::page).post(crate::docker::create),
        )
        .route("/docker/{id}/{action}", post(crate::docker::action))
        .route("/docker/{id}/logs", get(crate::docker::logs))
        .route(
            "/container/quota",
            get(crate::container_runtime::quota_page).post(crate::container_runtime::quota_update),
        )
        .route(
            "/registry/credentials",
            get(crate::container_runtime::registry_page)
                .post(crate::container_runtime::registry_create),
        )
        .route(
            "/registry/credentials/{id}/remove",
            post(crate::container_runtime::registry_remove),
        )
        .route(
            "/sites/{id}/ftp",
            get(crate::ftp::page).post(crate::ftp::create),
        )
        .route(
            "/sites/{site_id}/ftp/{account_id}/{action}",
            post(crate::ftp::action),
        )
        .route(
            "/login",
            get(login::login_page_handler).post(login::login_handler),
        )
        .route("/login/factor", post(login::login_factor_handler))
        .route("/logout", post(logout))
        .route("/settings", get(settings::page).post(settings::update))
        .route(
            "/settings/tokens",
            get(crate::api_tokens::page).post(crate::api_tokens::create),
        )
        .route(
            "/settings/tokens/{id}/{action}",
            post(crate::api_tokens::action),
        )
        .route(
            "/settings/notifications",
            get(crate::notifications::page).post(crate::notifications::create_channel),
        )
        .route(
            "/settings/notifications/channels/{id}/{action}",
            post(crate::notifications::channel_action),
        )
        .route(
            "/settings/notifications/subscriptions",
            post(crate::notifications::create_subscription),
        )
        .route(
            "/settings/notifications/subscriptions/{id}/disable",
            post(crate::notifications::disable_subscription),
        )
        .route("/settings/security", get(crate::two_factor::page))
        .route(
            "/settings/security/totp/enroll",
            post(crate::two_factor::enroll_totp),
        )
        .route(
            "/settings/security/totp/verify",
            post(crate::two_factor::verify_totp),
        )
        .route(
            "/settings/security/webauthn/register/begin",
            post(crate::two_factor::begin_webauthn),
        )
        .route(
            "/settings/security/webauthn/register/finish",
            post(crate::two_factor::finish_webauthn),
        )
        .route(
            "/settings/security/recovery/regenerate",
            post(crate::two_factor::regenerate_recovery),
        )
        .route(
            "/settings/security/factors/{id}/revoke",
            post(crate::two_factor::revoke_factor),
        )
        .route("/cron", get(cron::list))
        .route("/cron/new", get(cron::new_form))
        .route("/cron/jobs", post(cron::create))
        .route("/cron/jobs/{id}", get(cron::detail))
        .route("/cron/jobs/{id}/enable", post(cron::enable))
        .route("/cron/jobs/{id}/disable", post(cron::disable))
        .route("/cron/jobs/{id}/delete", post(cron::delete))
        .route("/cron/jobs/{id}/run", post(cron::run))
        .route("/cron/runs", get(cron::runs))
        .route("/cron/runs/{id}", get(cron::run_detail))
        .route("/backups", get(backups::page))
        .route("/backups/new", get(backups::new_form))
        .route("/backups/plans", post(backups::create))
        .route("/logs", get(logs::page))
        .route("/logs/entries", get(logs::entries))
        .route("/security", get(security::page))
        .route("/security/rules", post(security::create))
        .route("/services", get(crate::system_services::page))
        .route(
            "/services/{id}/actions",
            post(crate::system_services::action),
        )
        .route("/dns", get(crate::dns::page))
        .route("/dns/providers", post(crate::dns::create_provider))
        .route("/dns/providers/{id}/test", post(crate::dns::test_provider))
        .route(
            "/dns/providers/{id}/rotate",
            post(crate::dns::rotate_provider),
        )
        .route(
            "/dns/providers/{id}/disable",
            post(crate::dns::disable_provider),
        )
        .route(
            "/dns/providers/{id}/delete",
            post(crate::dns::delete_provider),
        )
        .route("/dns/providers/{id}/sync", post(crate::dns::sync_provider))
        .route("/dns/zones/{id}", get(crate::dns::zone_page))
        .route("/dns/zones/{id}/records", post(crate::dns::create_record))
        .route(
            "/dns/zones/{id}/records/{record_id}/update",
            post(crate::dns::update_record),
        )
        .route(
            "/dns/zones/{id}/records/{record_id}/delete",
            post(crate::dns::delete_record),
        )
        .route("/dns/zones/{id}/check", post(crate::dns::check_zone))
        .route("/mail", get(crate::mail::page))
        .route("/mail/domains", post(crate::mail::create_domain))
        .route("/software", get(crate::software_center::page))
        .route("/software/refresh", post(crate::software_center::refresh))
        .route("/software/entries/{id}", get(crate::software_center::entry))
        .route(
            "/software/entries/{id}/compatibility",
            get(crate::software_center::compatibility),
        )
        .route(
            "/software/components/{id}/preview",
            post(crate::software_center::preview),
        )
        .route(
            "/software/components/{id}/deploy",
            get(crate::software_center::deploy_form),
        )
        .route(
            "/software/components/{id}/install",
            post(crate::software_center::install_artifact),
        )
        .route(
            "/software/components/{id}/config",
            get(crate::software_center::config_page),
        )
        .route(
            "/software/components/{id}/config",
            post(crate::software_center::config_save),
        )
        .route(
            "/software/jobs/{id}/progress",
            get(crate::software_center::task_progress_fragment),
        )
        .route(
            "/software/components/{id}/{action}/preview",
            post(crate::software_center::preview_component_action),
        )
        .route(
            "/software/applications/preview",
            post(crate::software_center::preview_deployment),
        )
        .route(
            "/software/applications/plans/{digest}/execute",
            post(crate::software_center::execute_deployment),
        )
        .route(
            "/software/plans/{digest}/execute",
            post(crate::software_center::execute),
        )
        .route(
            "/software/jobs/{id}/cancel",
            post(crate::software_center::cancel),
        )
        .route(
            "/software/jobs/{id}/retry",
            post(crate::software_center::retry),
        )
        .route(
            "/software/jobs/{id}/rollback",
            post(crate::software_center::rollback),
        )
        .route("/sites", get(sites::list).post(sites::create))
        .route("/sites/new", get(sites::new_form))
        .route("/sites/{id}", get(sites::detail).delete(sites::delete))
        .route("/sites/{id}/enable", post(sites::enable))
        .route("/sites/{id}/disable", post(sites::disable))
        .route(
            "/sites/{id}/waf",
            get(crate::waf::page).post(crate::waf::save),
        )
        .route("/sites/{id}/waf/test", post(crate::waf::test_rule))
        .route("/users", get(users::list).post(users::create))
        .route("/users/new", get(users::new_form))
        .route("/users/{id}/role", post(users::change_role))
        .route("/users/{id}/enable", post(users::enable))
        .route("/users/{id}/disable", post(users::disable))
        .route("/users/{id}/password", post(users::reset_password))
        .route("/users/{id}", axum::routing::delete(users::delete))
        .route("/monitoring", get(monitoring::landing))
        .route("/monitoring/history", get(monitoring::history))
        .route("/monitoring/alerts", get(monitoring::alerts))
        .route("/ssl", get(ssl::list))
        .route("/ssl/new", get(ssl::new_form))
        .route("/ssl/issue", post(ssl::issue))
        .route(
            "/ssl/{domain}/force-https",
            axum::routing::patch(ssl::force_https),
        )
        .route("/ssl/{domain}/renew", post(ssl::renew))
        .route("/ssl/{domain}/revoke", post(ssl::revoke))
        .route("/ssl/{domain}", get(ssl::detail))
        .route(
            "/sites/{site_id}/files",
            get(files::list).post(files::write),
        )
        .route("/files", get(files::landing))
        .route("/sites/{site_id}/files/read", get(files::read))
        .route("/sites/{site_id}/files/write", post(files::write))
        .route("/sites/{site_id}/files/mkdir", post(files::mkdir))
        .route("/sites/{site_id}/files/rename", post(files::rename))
        .route("/sites/{site_id}/files/chmod", post(files::chmod))
        .route(
            "/sites/{site_id}/files/remove",
            axum::routing::delete(files::remove),
        )
        .route("/sites/{site_id}/files/upload", post(files::upload))
        .route("/databases", get(databases::list).post(databases::create))
        .route("/databases/new", get(databases::new_form))
        .route(
            "/databases/{id}",
            get(databases::detail).delete(databases::delete),
        )
        .route("/databases/{id}/password", post(databases::change_password))
        .route("/databases/{id}/reveal", post(databases::reveal))
        .route("/databases/{id}/pitr", get(crate::db_pitr::page))
        .route("/sites/{id}/staging", get(crate::site_staging::page))
        .route("/sites/{id}/cache", get(crate::site_cache_cdn::page))
        .route(
            "/cdn/integrations/{id}/purge",
            get(crate::site_cache_cdn::cdn_purge_page),
        )
        .route("/admin/branding", get(crate::themeable_ui::page))
        .route("/sites/{id}/collaborators", get(crate::collaborators::page))
        .route("/registry", get(crate::container_registry::page))
        .route("/audit", get(crate::audit::audit_index))
        .route("/audit/events", get(crate::audit::audit_list))
        .route("/marketplace", get(crate::plugin_marketplace::page))
        .route("/plugins", get(crate::plugin_extension::page))
        .route("/webmail", get(crate::webmail::index))
        .route("/webmail/folder/{name}", get(crate::webmail::folder))
        .route("/webmail/message/{id}", get(crate::webmail::message))
        .route(
            "/webmail/compose",
            get(crate::webmail::compose_form).post(crate::webmail::compose_send),
        )
        .route("/webmail/message/{id}/reply", get(crate::webmail::reply))
        .route(
            "/webmail/message/{id}/forward",
            get(crate::webmail::forward),
        )
        .route("/webmail/search", get(crate::webmail::search))
        .route(
            "/marketplace/{plugin_id}",
            get(crate::plugin_marketplace::page),
        )
        .route("/assets/htmx.min.js", get(assets::htmx_min_js))
        .route("/assets/app.css", get(assets::app_css))
        .route("/assets/tokens.css", get(assets::tokens_css))
        .layer(from_fn(audit_role_guard))
        .layer(from_fn_with_state(identity, session_middleware))
        .with_state(state)
}

/// Middleware that enforces the Owner/Admin role for the `/audit`
/// and `/marketplace` route groups. Other routes pass through
/// unchanged.
async fn audit_role_guard(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    use openpanel_api::extract::AuthSessionExt;
    let path = req.uri().path().to_string();
    let gated = path == "/audit"
        || path == "/audit/events"
        || path == "/marketplace"
        || path.starts_with("/marketplace/");
    if !gated {
        return next.run(req).await;
    }
    match req.auth_session() {
        Some(auth) if crate::audit::role_guard(auth.user.role()) => next.run(req).await,
        Some(_) => crate::audit::forbidden(),
        None => unauth_redirect(),
    }
}
