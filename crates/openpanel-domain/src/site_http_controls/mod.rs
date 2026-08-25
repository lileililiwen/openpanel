//! Per-site HTTP behaviour controls: error pages, redirects, protected
//! directories, hotlink protection, client-IP rules, MIME overrides,
//! and directory-index policy. Pure domain — no I/O.

use std::net::IpAddr;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised while validating or applying site HTTP controls.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SiteHttpError {
    /// The caller is not allowed to manage the site's controls.
    #[error("forbidden")]
    Forbidden,
    /// The requested site does not exist.
    #[error("site not found: {0}")]
    SiteNotFound(String),
    /// A control value violates a domain invariant.
    #[error("invalid site HTTP control: {0}")]
    Invalid(String),
    /// An override document resolves outside the site root.
    #[error("path outside site root")]
    PathOutsideSite,
    /// A redirect rule would loop against an existing rule.
    #[error("redirect loop detected")]
    RedirectLoop,
    /// Rendering or persistence failed.
    #[error("site HTTP controls failure: {0}")]
    Persistence(String),
}

/// Redirect response status codes supported by redirect rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedirectStatus {
    /// 301 Moved Permanently.
    MovedPermanently,
    /// 302 Found.
    Found,
    /// 307 Temporary Redirect.
    TemporaryRedirect,
    /// 308 Permanent Redirect.
    PermanentRedirect,
}

impl RedirectStatus {
    /// Numeric HTTP status code.
    pub fn code(self) -> u16 {
        match self {
            Self::MovedPermanently => 301,
            Self::Found => 302,
            Self::TemporaryRedirect => 307,
            Self::PermanentRedirect => 308,
        }
    }

    fn from_code(code: u16) -> Option<Self> {
        match code {
            301 => Some(Self::MovedPermanently),
            302 => Some(Self::Found),
            307 => Some(Self::TemporaryRedirect),
            308 => Some(Self::PermanentRedirect),
            _ => None,
        }
    }
}

/// Effect of a per-site client-IP rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpEffect {
    /// Permit matching source addresses.
    Allow,
    /// Reject matching source addresses with 403.
    Deny,
}

/// Override of the response body served for one HTTP status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorPageOverride {
    /// HTTP status being overridden (400–599, excluding 401).
    pub status: u16,
    /// Site-root-relative document path, e.g. `/errors/404.html`.
    pub document_path: String,
}

/// One ordered prefix redirect rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedirectRule {
    /// Lower ordinals apply first; ties break on source prefix.
    pub ordinal: u16,
    /// Absolute path prefix matched against the request path.
    pub source_prefix: String,
    /// Destination path or absolute URL.
    pub destination: String,
    /// Redirect status to emit.
    pub status: RedirectStatus,
}

/// One basic-auth account inside a protected directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasicAuthAccount {
    /// Account name (htpasswd user column).
    pub name: String,
    /// bcrypt hash in `$2a$`/`$2b$`/`$2y$` form. Plaintext is never stored.
    pub password_hash: String,
}

/// A path prefix protected with HTTP basic auth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedDir {
    /// Absolute path prefix to protect.
    pub path_prefix: String,
    /// Realm string presented to clients (no quotes/backslashes).
    pub realm: String,
    /// Accounts allowed access.
    pub accounts: Vec<BasicAuthAccount>,
}

/// Referer-based hotlink protection policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotlinkPolicy {
    /// Referer host suffixes that are always allowed.
    pub allowed_referers: Vec<String>,
    /// When true, referers not on the allow-list receive 403.
    pub default_deny: bool,
}

/// One ordered per-site client-IP allow/deny rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientIpRule {
    /// Lower ordinals evaluate first.
    pub ordinal: u16,
    /// CIDR string, e.g. `203.0.113.0/24` or `2001:db8::/32`.
    pub cidr: String,
    /// Action for matching addresses.
    pub effect: IpEffect,
}

/// Per-extension MIME type override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MimeOverride {
    /// File extension without the leading dot, e.g. `webmanifest`.
    pub extension: String,
    /// MIME type token, e.g. `application/manifest+json`.
    pub mime_type: String,
}

/// Directory index ordering and autoindex toggle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexPolicy {
    /// Ordered candidate index file names.
    pub order: Vec<String>,
    /// Whether directory listings are served when no index matches.
    pub autoindex: bool,
}

fn invalid(message: impl Into<String>) -> SiteHttpError {
    SiteHttpError::Invalid(message.into())
}

fn validate_site_path(value: &str, max: usize) -> Result<(), SiteHttpError> {
    if value.is_empty()
        || !value.starts_with('/')
        || value.len() > max
        || value.contains('\0')
        || value.contains('\\')
        || value.split('/').any(|segment| segment == "..")
    {
        return Err(SiteHttpError::PathOutsideSite);
    }
    Ok(())
}

fn validate_plain(value: &str, max: usize) -> Result<(), SiteHttpError> {
    if value.is_empty()
        || value.len() > max
        || value
            .bytes()
            .any(|byte| matches!(byte, b';' | b'{' | b'}' | b'"' | b'\n' | b'\r' | 0))
    {
        return Err(invalid("value contains unsafe characters"));
    }
    Ok(())
}

impl ErrorPageOverride {
    /// Validate status range and document path containment.
    pub fn validate(&self) -> Result<(), SiteHttpError> {
        if !(400..=599).contains(&self.status)
            || self.status == 401
            || RedirectStatus::from_code(self.status).is_some()
        {
            return Err(invalid(
                "error-page status must be a 4xx/5xx code other than 401 and non-redirect",
            ));
        }
        validate_site_path(&self.document_path, 512)
    }
}

impl RedirectRule {
    /// Validate prefixes, destination, and ordinal.
    pub fn validate(&self) -> Result<(), SiteHttpError> {
        validate_site_path(&self.source_prefix, 512)?;
        if self.source_prefix.matches('%').count() > 0 {
            return Err(invalid("source prefix must not contain percent escapes"));
        }
        let destination_ok = self.destination.starts_with('/')
            && validate_site_path(&self.destination, 1024).is_ok()
            || (self.destination.starts_with("http://")
                || self.destination.starts_with("https://"))
                && validate_plain(&self.destination, 1024).is_ok();
        if !destination_ok {
            return Err(invalid(
                "destination must be a site-root-relative path or an http(s) URL",
            ));
        }
        Ok(())
    }

    /// Whether this rule's source equals another rule's destination
    /// prefix (or vice versa), which would create a redirect loop.
    pub fn loops_with(&self, other: &RedirectRule) -> bool {
        self.source_prefix == other.destination || other.source_prefix == self.destination
    }
}

impl BasicAuthAccount {
    /// Validate name charset and bcrypt hash shape.
    pub fn validate(&self) -> Result<(), SiteHttpError> {
        if self.name.is_empty()
            || self.name.len() > 64
            || !self.name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'@' | b'-')
            })
        {
            return Err(invalid("invalid basic-auth account name"));
        }
        let hash = &self.password_hash;
        let shaped = hash.len() >= 59
            && (hash.starts_with("$2a$") || hash.starts_with("$2b$") || hash.starts_with("$2y$"));
        if !shaped {
            return Err(invalid("password must be a bcrypt hash, never plaintext"));
        }
        Ok(())
    }
}

impl ProtectedDir {
    /// Validate prefix, realm, and every account.
    pub fn validate(&self) -> Result<(), SiteHttpError> {
        validate_site_path(&self.path_prefix, 512)?;
        if self.accounts.is_empty() || self.accounts.len() > 64 {
            return Err(invalid("a protected directory needs 1–64 accounts"));
        }
        validate_plain(&self.realm, 128)?;
        for account in &self.accounts {
            account.validate()?;
        }
        Ok(())
    }
}

impl HotlinkPolicy {
    /// Validate the allow-list entries as host suffixes.
    pub fn validate(&self) -> Result<(), SiteHttpError> {
        if self.allowed_referers.len() > 64 {
            return Err(invalid("at most 64 allowed referers"));
        }
        for referer in &self.allowed_referers {
            if referer.is_empty()
                || referer.len() > 253
                || referer.bytes().any(|byte| {
                    !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'*' | b':'))
                })
            {
                return Err(invalid("allowed referer must be a host suffix"));
            }
        }
        Ok(())
    }
}

impl ClientIpRule {
    /// Validate ordinal, CIDR syntax, and effect.
    pub fn validate(&self) -> Result<(), SiteHttpError> {
        let (address, prefix_len) = match self.cidr.split_once('/') {
            Some((address, prefix)) => (
                address
                    .parse::<IpAddr>()
                    .map_err(|_| invalid("invalid CIDR address"))?,
                prefix
                    .parse::<u8>()
                    .map_err(|_| invalid("invalid CIDR prefix length"))?,
            ),
            None => (
                self.cidr
                    .parse::<IpAddr>()
                    .map_err(|_| invalid("invalid CIDR address"))?,
                match self.cidr.parse::<IpAddr>() {
                    Ok(IpAddr::V4(_)) => 32,
                    Ok(IpAddr::V6(_)) => 128,
                    Err(_) => return Err(invalid("invalid CIDR")),
                },
            ),
        };
        let max_prefix = match address {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix_len > max_prefix {
            return Err(invalid("CIDR prefix length exceeds address size"));
        }
        Ok(())
    }
}

impl MimeOverride {
    /// Validate extension and MIME token charsets.
    pub fn validate(&self) -> Result<(), SiteHttpError> {
        if self.extension.is_empty()
            || self.extension.len() > 32
            || self.extension.starts_with('.')
            || !self
                .extension
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        {
            return Err(invalid(
                "extension must be lowercase alphanumeric without dot",
            ));
        }
        let mime_ok = self.mime_type.len() <= 128 && {
            let mut parts = self.mime_type.splitn(2, '/');
            let type_part = parts.next().unwrap_or_default();
            let sub_part = parts.next().unwrap_or_default();
            !type_part.is_empty()
                && !sub_part.is_empty()
                && self.mime_type.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'+' | b'.' | b'_')
                })
        };
        if !mime_ok {
            return Err(invalid("mime_type must look like type/subtype"));
        }
        Ok(())
    }
}

impl IndexPolicy {
    /// Validate the ordered index list.
    pub fn validate(&self) -> Result<(), SiteHttpError> {
        if self.order.is_empty() || self.order.len() > 16 {
            return Err(invalid("index order needs 1–16 entries"));
        }
        for entry in &self.order {
            if entry.is_empty()
                || entry.len() > 128
                || entry.starts_with('/')
                || entry
                    .bytes()
                    .any(|byte| matches!(byte, b'/' | b';' | b'{' | b'}' | 0))
            {
                return Err(invalid("index entries must be plain file names"));
            }
        }
        Ok(())
    }
}

/// Versioned HTTP-controls document attached to one site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteHttpControls {
    site_id: Uuid,
    version: u64,
    #[serde(default)]
    error_pages: Vec<ErrorPageOverride>,
    #[serde(default)]
    redirects: Vec<RedirectRule>,
    #[serde(default)]
    protected_dirs: Vec<ProtectedDir>,
    #[serde(default)]
    hotlink: Option<HotlinkPolicy>,
    #[serde(default)]
    ip_rules: Vec<ClientIpRule>,
    #[serde(default)]
    mime_overrides: Vec<MimeOverride>,
    #[serde(default)]
    index_policy: Option<IndexPolicy>,
}

/// Raw per-site control sections accepted by
/// [`SiteHttpControls::new`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SiteHttpControlsInput {
    /// Error-page overrides.
    pub error_pages: Vec<ErrorPageOverride>,
    /// Prefix redirect rules.
    pub redirects: Vec<RedirectRule>,
    /// Basic-auth protected directories.
    pub protected_dirs: Vec<ProtectedDir>,
    /// Hotlink protection policy.
    pub hotlink: Option<HotlinkPolicy>,
    /// Ordered client-IP rules.
    pub ip_rules: Vec<ClientIpRule>,
    /// MIME type overrides.
    pub mime_overrides: Vec<MimeOverride>,
    /// Directory-index policy.
    pub index_policy: Option<IndexPolicy>,
}

impl SiteHttpControls {
    /// Validate and construct a deterministic controls document.
    pub fn new(
        site_id: Uuid,
        version: u64,
        input: SiteHttpControlsInput,
    ) -> Result<Self, SiteHttpError> {
        let SiteHttpControlsInput {
            error_pages,
            redirects,
            protected_dirs,
            hotlink,
            ip_rules,
            mime_overrides,
            index_policy,
        } = input;
        if version == 0 {
            return Err(invalid("version must be positive"));
        }
        if error_pages.len() > 32 {
            return Err(invalid("at most 32 error-page overrides"));
        }
        let mut seen_status = std::collections::HashSet::new();
        for page in &error_pages {
            page.validate()?;
            if !seen_status.insert(page.status) {
                return Err(invalid("duplicate error-page status"));
            }
        }
        if redirects.len() > 64 {
            return Err(invalid("at most 64 redirect rules"));
        }
        for rule in &redirects {
            rule.validate()?;
        }
        let mut ordered = redirects;
        ordered.sort_by(|a, b| (a.ordinal, &a.source_prefix).cmp(&(b.ordinal, &b.source_prefix)));
        for pair in ordered.windows(2) {
            if pair[0].loops_with(&pair[1]) {
                return Err(SiteHttpError::RedirectLoop);
            }
        }
        if protected_dirs.len() > 32 {
            return Err(invalid("at most 32 protected directories"));
        }
        let mut seen_prefix = std::collections::HashSet::new();
        for dir in &protected_dirs {
            dir.validate()?;
            if !seen_prefix.insert(dir.path_prefix.clone()) {
                return Err(invalid("duplicate protected directory prefix"));
            }
        }
        if let Some(policy) = &hotlink {
            policy.validate()?;
        }
        if ip_rules.len() > 64 {
            return Err(invalid("at most 64 client-IP rules"));
        }
        for rule in &ip_rules {
            rule.validate()?;
        }
        if mime_overrides.len() > 64 {
            return Err(invalid("at most 64 MIME overrides"));
        }
        let mut seen_ext = std::collections::HashSet::new();
        for over in &mime_overrides {
            over.validate()?;
            if !seen_ext.insert(over.extension.clone()) {
                return Err(invalid("duplicate MIME override extension"));
            }
        }
        if let Some(policy) = &index_policy {
            policy.validate()?;
        }
        Ok(Self {
            site_id,
            version,
            error_pages,
            redirects: ordered,
            protected_dirs,
            hotlink,
            ip_rules,
            mime_overrides,
            index_policy,
        })
    }

    /// Empty initial document for a site.
    pub fn empty(site_id: Uuid) -> Self {
        Self {
            site_id,
            version: 1,
            error_pages: Vec::new(),
            redirects: Vec::new(),
            protected_dirs: Vec::new(),
            hotlink: None,
            ip_rules: Vec::new(),
            mime_overrides: Vec::new(),
            index_policy: None,
        }
    }

    /// Revalidate a deserialized document and restore deterministic order.
    pub fn validated(self) -> Result<Self, SiteHttpError> {
        Self::new(
            self.site_id,
            self.version,
            SiteHttpControlsInput {
                error_pages: self.error_pages,
                redirects: self.redirects,
                protected_dirs: self.protected_dirs,
                hotlink: self.hotlink,
                ip_rules: self.ip_rules,
                mime_overrides: self.mime_overrides,
                index_policy: self.index_policy,
            },
        )
    }

    /// Site identifier.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Optimistic document version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Error-page overrides.
    pub fn error_pages(&self) -> &[ErrorPageOverride] {
        &self.error_pages
    }

    /// Redirect rules in deterministic order.
    pub fn redirects(&self) -> &[RedirectRule] {
        &self.redirects
    }

    /// Protected directories.
    pub fn protected_dirs(&self) -> &[ProtectedDir] {
        &self.protected_dirs
    }

    /// Hotlink policy, when configured.
    pub fn hotlink(&self) -> Option<&HotlinkPolicy> {
        self.hotlink.as_ref()
    }

    /// Client-IP rules.
    pub fn ip_rules(&self) -> &[ClientIpRule] {
        &self.ip_rules
    }

    /// MIME overrides.
    pub fn mime_overrides(&self) -> &[MimeOverride] {
        &self.mime_overrides
    }

    /// Directory-index policy, when configured.
    pub fn index_policy(&self) -> Option<&IndexPolicy> {
        self.index_policy.as_ref()
    }

    /// Whether any section carries content.
    pub fn is_empty(&self) -> bool {
        self.error_pages.is_empty()
            && self.redirects.is_empty()
            && self.protected_dirs.is_empty()
            && self.hotlink.is_none()
            && self.ip_rules.is_empty()
            && self.mime_overrides.is_empty()
            && self.index_policy.is_none()
    }
}

/// Persistence port for per-site HTTP-controls documents.
#[async_trait]
pub trait SiteHttpRepository: Send + Sync + 'static {
    /// Load the controls document for a site.
    async fn get(&self, site_id: Uuid) -> Result<Option<SiteHttpControls>, RepoError>;
    /// Atomically replace the controls document for a site.
    async fn put(&self, controls: &SiteHttpControls) -> Result<(), RepoError>;
}

#[cfg(test)]
mod tests;
