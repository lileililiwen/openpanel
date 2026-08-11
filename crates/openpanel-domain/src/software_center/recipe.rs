//! Rich recipe schema for the Software Center catalog.
//!
//! These types are *data only* — they describe what a catalog entry looks
//! like and how it is parsed, validated, and queried. They never execute
//! code, never carry command lines, and never accept shell syntax. The
//! catalog stays declarative so that the panel can render it, search it,
//! and act on it safely.
#![allow(missing_docs)]

use std::{
    collections::BTreeSet,
    fmt,
    path::{Component, Path},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::SupportedPlatform;

/// Soft error used across recipe validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeError {
    /// A field carries an invalid value.
    Invalid(&'static str),
    /// A reference points to a missing or unsupported peer.
    Unknown,
    /// A field exceeds its bounded length.
    TooLong,
    /// A manifest is structurally valid but its contents are mutually
    /// inconsistent.
    Inconsistent(&'static str),
}

impl fmt::Display for RecipeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(field) => write!(formatter, "invalid recipe field: {field}"),
            Self::Unknown => formatter.write_str("unknown recipe reference"),
            Self::TooLong => formatter.write_str("recipe field too long"),
            Self::Inconsistent(reason) => write!(formatter, "inconsistent recipe: {reason}"),
        }
    }
}

impl std::error::Error for RecipeError {}

/// Closed top-level category for a catalog entry.
///
/// The set is intentionally closed so the storefront tab strip and the
/// `category=` filter always agree. New categories require an explicit
/// decision in the change that adds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    /// One-click web applications (WordPress, Drupal, ...).
    OneClick,
    /// PHP-FPM runtimes and extensions.
    Runtime,
    /// Database servers (MySQL, MariaDB, PostgreSQL, ...).
    Database,
    /// In-memory caches and key/value stores.
    Cache,
    /// HTTP servers and reverse proxies.
    WebServer,
    /// Mail transfer agents and IMAP servers.
    Mail,
    /// Operator tools (phpMyAdmin, Adminer, web terminal, ...).
    Tools,
    /// Security utilities (fail2ban, mod_security, ...).
    Security,
    /// Anything that does not fit the above.
    Other,
}

impl Category {
    /// Stable kebab-case slug used in URLs and query parameters.
    pub fn slug(self) -> &'static str {
        match self {
            Self::OneClick => "one-click",
            Self::Runtime => "runtime",
            Self::Database => "database",
            Self::Cache => "cache",
            Self::WebServer => "web-server",
            Self::Mail => "mail",
            Self::Tools => "tools",
            Self::Security => "security",
            Self::Other => "other",
        }
    }

    /// Display label shown in the storefront.
    pub fn label(self) -> &'static str {
        match self {
            Self::OneClick => "One-click",
            Self::Runtime => "Runtimes",
            Self::Database => "Databases",
            Self::Cache => "Caching",
            Self::WebServer => "Web servers",
            Self::Mail => "Mail",
            Self::Tools => "Tools",
            Self::Security => "Security",
            Self::Other => "Other",
        }
    }

    /// Closed enumeration of every supported category in canonical order.
    pub fn all() -> &'static [Category] {
        &[
            Self::OneClick,
            Self::Runtime,
            Self::Database,
            Self::Cache,
            Self::WebServer,
            Self::Mail,
            Self::Tools,
            Self::Security,
            Self::Other,
        ]
    }
}

impl FromStr for Category {
    type Err = RecipeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "one-click" => Ok(Self::OneClick),
            "runtime" => Ok(Self::Runtime),
            "database" => Ok(Self::Database),
            "cache" => Ok(Self::Cache),
            "web-server" => Ok(Self::WebServer),
            "mail" => Ok(Self::Mail),
            "tools" => Ok(Self::Tools),
            "security" => Ok(Self::Security),
            "other" => Ok(Self::Other),
            _ => Err(RecipeError::Invalid("category")),
        }
    }
}

/// Validated kebab-case tag (1..=32 chars, lowercase, digits, `-`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Tag(String);

impl Tag {
    /// Maximum length of a tag in characters.
    pub const MAX_LENGTH: usize = 32;

    /// Validate a raw tag string.
    pub fn new(value: &str) -> Result<Self, RecipeError> {
        if value.is_empty()
            || value.len() > Self::MAX_LENGTH
            || !value.is_ascii()
            || value.starts_with('-')
            || value.ends_with('-')
            || value.contains("--")
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(RecipeError::Invalid("tag"));
        }
        Ok(Self(value.to_owned()))
    }

    /// Tag text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Tag {
    type Error = RecipeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<Tag> for String {
    fn from(tag: Tag) -> Self {
        tag.0
    }
}

impl AsRef<str> for Tag {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Validated SPDX-style license identifier.
///
/// The set is not exhaustive; we accept the short SPDX expression form
/// (`MIT`, `GPL-2.0-only`, `Apache-2.0`, `BSD-2-Clause`, ...) with the
/// constraint that it is a single identifier up to 64 chars. Compound
/// expressions like `MIT OR Apache-2.0` are rejected in v1 to keep the
/// license cell short and sortable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct License(String);

impl License {
    /// Maximum length of a license identifier.
    pub const MAX_LENGTH: usize = 64;

    /// Validate a license identifier.
    pub fn new(value: &str) -> Result<Self, RecipeError> {
        if value.is_empty() || value.len() > Self::MAX_LENGTH || !value.is_ascii() {
            return Err(RecipeError::Invalid("license"));
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'+'))
        {
            return Err(RecipeError::Invalid("license"));
        }
        Ok(Self(value.to_owned()))
    }

    /// License text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for License {
    type Error = RecipeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<License> for String {
    fn from(license: License) -> Self {
        license.0
    }
}

/// Validated HTTPS homepage URL (no userinfo, no fragment, no query).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Homepage(String);

impl Homepage {
    /// Maximum URL length.
    pub const MAX_LENGTH: usize = 256;

    /// Validate a homepage URL.
    pub fn new(value: &str) -> Result<Self, RecipeError> {
        if value.is_empty() || value.len() > Self::MAX_LENGTH {
            return Err(RecipeError::Invalid("homepage"));
        }
        let parsed = url::Url::parse(value).map_err(|_| RecipeError::Invalid("homepage"))?;
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(RecipeError::Invalid("homepage"));
        }
        Ok(Self(value.to_owned()))
    }

    /// URL text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Homepage {
    type Error = RecipeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<Homepage> for String {
    fn from(homepage: Homepage) -> Self {
        homepage.0
    }
}

/// Closed entry kind discriminator. The kernel and the storefront branch
/// on this to choose between `PackageId` and `PinnedArtifact`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    /// Host package or runtime installed by the package manager.
    System,
    /// Deployable web application installed through the artifact pipeline.
    Web,
    /// Operator tool — system install but with a Tools category.
    Tool,
}

impl EntryKind {
    /// Stable text label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Web => "web",
            Self::Tool => "tool",
        }
    }
}

/// One supported version of an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionSpec {
    /// Semver-like version string (e.g. `8.3.0`, `1.0.0-rc.1`).
    pub version: String,
    /// Disk delta in bytes the recipe expects to consume (download + install).
    #[serde(default)]
    pub size_bytes: u64,
    /// Optional changelog or release-note URL (validated when present).
    #[serde(default)]
    pub changelog_url: Option<Homepage>,
    /// For `Web` entries: pinned artifact URL and digest.
    #[serde(default)]
    pub artifact: Option<ArtifactPin>,
    /// For `System` entries: fixed package identifiers.
    #[serde(default)]
    pub packages: Vec<String>,
    /// PHP runtime versions this version supports (web entries only).
    #[serde(default)]
    pub supports_php: Vec<String>,
    /// Release date as an ISO 8601 string (e.g. `2024-04-09`).
    #[serde(default)]
    pub released_at: Option<String>,
}

impl VersionSpec {
    /// Validate the version string and any optional artifact pin.
    pub fn validate(&self) -> Result<(), RecipeError> {
        if self.version.is_empty() || self.version.len() > 64 {
            return Err(RecipeError::Invalid("version"));
        }
        if self
            .version
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+')))
        {
            return Err(RecipeError::Invalid("version"));
        }
        if let Some(artifact) = &self.artifact {
            artifact.validate()?;
        }
        for package in &self.packages {
            if package.is_empty() || package.len() > 128 {
                return Err(RecipeError::Invalid("package"));
            }
            if package.bytes().any(|byte| {
                !(byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'+' | b'-'))
            }) {
                return Err(RecipeError::Invalid("package"));
            }
        }
        if self.supports_php.len() > 16 {
            return Err(RecipeError::TooLong);
        }
        Ok(())
    }
}

/// Pinned artifact metadata for a `Web` entry version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPin {
    /// HTTPS URL the artifact is downloaded from.
    pub url: Homepage,
    /// SHA-256 digest of the artifact bytes (lowercase hex, 64 chars).
    pub sha256: String,
    /// Top-level directory inside the archive that is stripped on extract.
    pub archive_root: String,
    /// Archive type. Only `tar.gz` is supported in v1.
    pub archive_type: String,
    /// Optional SHA-1 digest (WordPress uses this for verification).
    #[serde(default)]
    pub sha1: Option<String>,
}

impl ArtifactPin {
    /// Validate the artifact pin.
    pub fn validate(&self) -> Result<(), RecipeError> {
        let is_single_file = self.archive_type == "file";
        if !is_single_file
            && (self.archive_root.is_empty()
                || self.archive_root.len() > 128
                || self.archive_root.contains('/')
                || self.archive_root.contains('\\')
                || self
                    .archive_root
                    .bytes()
                    .any(|byte| byte.is_ascii_control())
                || Path::new(&self.archive_root)
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_))))
        {
            return Err(RecipeError::Invalid("archive_root"));
        }
        if !matches!(
            self.archive_type.as_str(),
            "tar.gz" | "tar.bz2" | "zip" | "file"
        ) {
            return Err(RecipeError::Invalid("archive_type"));
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(RecipeError::Invalid("sha256"));
        }
        if let Some(sha1) = &self.sha1
            && (sha1.len() != 40 || !sha1.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(RecipeError::Invalid("sha1"));
        }
        Ok(())
    }
}

/// Whole catalog entry as it appears in a manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntryRecipe {
    /// Stable identifier (validated via `CatalogId`).
    pub id: String,
    /// Display name (1..=128 chars).
    pub name: String,
    /// One-line description (1..=256 chars).
    pub description: String,
    /// Long-form description with paragraphs separated by `\n\n`.
    #[serde(default)]
    pub long_description: String,
    /// Top-level category.
    pub category: Category,
    /// Entry kind.
    pub kind: EntryKind,
    /// At least one tag.
    pub tags: Vec<Tag>,
    /// SPDX license identifier.
    pub license: License,
    /// Developer / maintainer string (1..=128 chars).
    pub developer: String,
    /// HTTPS homepage.
    pub homepage: Homepage,
    /// At least one version. The first is treated as `latest`.
    pub versions: Vec<VersionSpec>,
    /// Supported host tuples for this entry. An empty list is
    /// interpreted as "supported on every host OpenPanel recognizes".
    #[serde(default)]
    pub platforms: Vec<SupportedPlatform>,
    /// Entry IDs this entry depends on.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Entry IDs this entry conflicts with.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Icon key (resolved against the storefront icon set).
    #[serde(default)]
    pub icon: Option<String>,
    /// Optional screenshot URL.
    #[serde(default)]
    pub screenshot: Option<Homepage>,
}

impl CatalogEntryRecipe {
    /// Validate the entry. `known_ids` lists every other entry ID in the
    /// same manifest so dependency and conflict references can be checked.
    pub fn validate(&self, known_ids: &BTreeSet<String>) -> Result<(), RecipeError> {
        super::CatalogId::new(&self.id).map_err(|_| RecipeError::Invalid("id"))?;
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(RecipeError::Invalid("name"));
        }
        if self.description.trim().is_empty() || self.description.len() > 256 {
            return Err(RecipeError::Invalid("description"));
        }
        if self.long_description.len() > 8192 {
            return Err(RecipeError::TooLong);
        }
        if self.developer.trim().is_empty() || self.developer.len() > 128 {
            return Err(RecipeError::Invalid("developer"));
        }
        if self.tags.is_empty() || self.tags.len() > 16 {
            return Err(RecipeError::Invalid("tags"));
        }
        if self.versions.is_empty() || self.versions.len() > 32 {
            return Err(RecipeError::Invalid("versions"));
        }
        for version in &self.versions {
            version.validate()?;
        }
        match self.kind {
            EntryKind::System | EntryKind::Tool => {
                for version in &self.versions {
                    if version.packages.is_empty() {
                        return Err(RecipeError::Inconsistent("system versions need packages"));
                    }
                    if version.artifact.is_some() {
                        return Err(RecipeError::Inconsistent(
                            "system versions have no artifact",
                        ));
                    }
                }
            }
            EntryKind::Web => {
                for version in &self.versions {
                    if version.artifact.is_none() {
                        return Err(RecipeError::Inconsistent("web versions need artifact"));
                    }
                    if !version.packages.is_empty() {
                        return Err(RecipeError::Inconsistent(
                            "web versions do not declare system packages",
                        ));
                    }
                }
            }
        }
        if self.dependencies.len() > 16 || self.conflicts.len() > 16 {
            return Err(RecipeError::TooLong);
        }
        for dep in &self.dependencies {
            if !known_ids.contains(dep) {
                return Err(RecipeError::Unknown);
            }
            if dep == &self.id {
                return Err(RecipeError::Inconsistent("self dependency"));
            }
        }
        for conflict in &self.conflicts {
            if !known_ids.contains(conflict) {
                return Err(RecipeError::Unknown);
            }
            if conflict == &self.id {
                return Err(RecipeError::Inconsistent("self conflict"));
            }
        }
        Ok(())
    }

    /// Latest version, which is the first entry in the `versions` list.
    pub fn latest_version(&self) -> &VersionSpec {
        &self.versions[0]
    }
}

/// Whole manifest as it appears in a signed envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogManifest {
    /// Schema version. v1.
    pub schema: u32,
    /// Manifest issuance timestamp (RFC 3339).
    pub issued_at: String,
    /// Manifest source URL.
    pub source_url: String,
    /// Entries in stable (insertion) order.
    pub entries: Vec<CatalogEntryRecipe>,
}

impl CatalogManifest {
    /// Validate the entire manifest and every entry.
    pub fn validate(&self) -> Result<(), RecipeError> {
        if self.schema != 1 {
            return Err(RecipeError::Invalid("schema"));
        }
        if self.entries.is_empty() || self.entries.len() > 256 {
            return Err(RecipeError::Invalid("entries"));
        }
        let mut ids = BTreeSet::new();
        for entry in &self.entries {
            if !ids.insert(entry.id.clone()) {
                return Err(RecipeError::Inconsistent("duplicate id"));
            }
        }
        for entry in &self.entries {
            entry.validate(&ids)?;
        }
        Ok(())
    }

    /// SHA-256 digest of the canonical JSON of the manifest. Used as the
    /// snapshot identifier in the `Provenance` record.
    pub fn digest(&self) -> String {
        let canonical = serde_json::to_vec(self).unwrap_or_default();
        hex::encode(Sha256::digest(canonical))
    }
}

/// Source provenance for the active snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Manifest source URL.
    pub source_url: String,
    /// SHA-256 digest of the canonical manifest bytes.
    pub manifest_digest: String,
    /// Activation timestamp (RFC 3339).
    pub activated_at: String,
    /// Number of entries in the activated snapshot.
    pub entry_count: usize,
    /// True when the snapshot was activated from the embedded recovery
    /// seed rather than a remote manifest.
    pub embedded: bool,
}

/// Search/filter query for the storefront.
#[derive(Debug, Clone, Default)]
pub struct CatalogQuery {
    /// Free-text query, matched against name, description, tags, developer.
    pub text: Option<String>,
    /// Restrict to one or more categories.
    pub categories: Vec<Category>,
    /// Restrict to one or more tags.
    pub tags: Vec<Tag>,
    /// Restrict to one of the entry kinds.
    pub kind: Option<EntryKind>,
    /// When true, only return entries that are locally installed.
    pub installed_only: bool,
    /// When true, only return entries with an update available.
    pub update_available_only: bool,
    /// Sort order.
    pub sort: CatalogSort,
    /// Page index (0-based) and page size.
    pub page: usize,
    pub page_size: usize,
}

/// Sort key for catalog queries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CatalogSort {
    /// Default — case-insensitive name.
    #[default]
    Name,
    /// Most recent activation time first.
    Recent,
    /// Largest install size first.
    Size,
}

/// A single search hit as returned by `SoftwareCenterService::search`.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogHit {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: Category,
    pub kind: EntryKind,
    pub license: String,
    pub developer: String,
    pub latest_version: String,
    pub tags: Vec<String>,
    pub install_state: String,
    pub icon: Option<String>,
    pub size_bytes: u64,
    /// `true` when the entry's recipe carries the documented
    /// placeholder SHA-256 sentinel. The storefront disables the
    /// Install button for these entries when the fail-closed gate is
    /// on; operators must run `software refresh` against a remote
    /// catalog that pins a real digest before installing.
    pub placeholder_digest: bool,
}

/// Catalog filter + pagination summary returned to the storefront.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogSearchPage {
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub sort: &'static str,
    pub hits: Vec<CatalogHit>,
}

impl CatalogSearchPage {
    /// Build an empty page with the given query parameters.
    pub fn empty(query: &CatalogQuery) -> Self {
        Self {
            total: 0,
            page: query.page,
            page_size: query.page_size,
            sort: sort_label(query.sort),
            hits: Vec::new(),
        }
    }
}

fn sort_label(sort: CatalogSort) -> &'static str {
    match sort {
        CatalogSort::Name => "name",
        CatalogSort::Recent => "recent",
        CatalogSort::Size => "size",
    }
}
