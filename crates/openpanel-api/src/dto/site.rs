use chrono::{DateTime, Utc};
use openpanel_domain::{Site, SiteStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateSiteRequest {
    pub owner_id: Option<Uuid>,
    pub primary_domain: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub php_enabled: bool,
    #[serde(default)]
    pub php_version: Option<String>,
    #[serde(default)]
    pub document_root: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchSiteRequest {
    pub owner_id: Option<Uuid>,
    pub aliases: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct SiteDto {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub primary_domain: String,
    pub aliases: Vec<String>,
    pub document_root: String,
    pub php_enabled: bool,
    pub php_version: Option<String>,
    pub status: SiteStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SiteDto {
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
