use chrono::{DateTime, Utc};
use openpanel_domain::{Site, SiteStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request body for `POST /sites`.
#[derive(Debug, Deserialize)]
pub struct CreateSiteRequest {
    /// Optional explicit owner; defaults to the caller when omitted.
    pub owner_id: Option<Uuid>,
    /// Primary hostname the site should serve (e.g. `example.com`).
    pub primary_domain: String,
    /// Additional hostname aliases pointing to the same site.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Whether PHP-FPM should be enabled for this site.
    #[serde(default)]
    pub php_enabled: bool,
    /// Desired PHP version (e.g. `"8.3"`); required when `php_enabled` is true.
    #[serde(default)]
    pub php_version: Option<String>,
    /// Document root path relative to the site root.
    #[serde(default)]
    pub document_root: Option<String>,
}

/// Request body for `PATCH /sites/{id}` to apply partial updates.
#[derive(Debug, Deserialize)]
pub struct PatchSiteRequest {
    /// New owning user's id, if ownership should change.
    pub owner_id: Option<Uuid>,
    /// Replacement alias list (full replace, not merge).
    pub aliases: Option<Vec<String>>,
}

/// Public-facing projection of a [`Site`] returned to clients.
#[derive(Debug, Serialize)]
pub struct SiteDto {
    /// Unique site identifier.
    pub id: Uuid,
    /// Owning user's id.
    pub owner_id: Uuid,
    /// Primary hostname served by the site.
    pub primary_domain: String,
    /// Additional hostname aliases.
    pub aliases: Vec<String>,
    /// Effective document root path.
    pub document_root: String,
    /// Whether PHP is enabled for this site.
    pub php_enabled: bool,
    /// Active PHP version, when PHP is enabled.
    pub php_version: Option<String>,
    /// Current lifecycle state.
    pub status: SiteStatus,
    /// Timestamp the site was provisioned.
    pub created_at: DateTime<Utc>,
    /// Timestamp of the most recent configuration change.
    pub updated_at: DateTime<Utc>,
}

impl SiteDto {
    /// Projects a domain [`Site`] into its wire DTO form.
    pub fn from_site(site: &Site) -> Self {
        Self {
            id: site.id(),
            owner_id: site.owner_id(),
            primary_domain: site.primary_domain().to_string(),
            aliases: site.aliases().to_vec(),
            document_root: site.document_root().to_string(),
            php_enabled: site.php_enabled(),
            php_version: site.php_version().map(str::to_string),
            status: site.status(),
            created_at: site.created_at(),
            updated_at: site.updated_at(),
        }
    }
}
