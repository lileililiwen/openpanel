//! SQLite-backed adapter for the themeable-ui bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    HexColor, Palette, PanelDomain, ThemeOverride, ThemeableUiError, ThemeableUiRepository,
    Typography,
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// SQLite-backed repository.
#[derive(Clone)]
pub struct SqliteThemeableUiRepository {
    pool: Pool<Sqlite>,
}

impl SqliteThemeableUiRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn decode_palette(json: &str) -> Result<Palette, ThemeableUiError> {
    #[derive(serde::Deserialize)]
    struct Pj {
        color_fg: String,
        color_bg: String,
        color_accent: String,
        contrast_min: f64,
    }
    let pj: Pj = serde_json::from_str(json)
        .map_err(|e| ThemeableUiError::Persistence(format!("palette json: {e}")))?;
    let fg = HexColor::parse(&pj.color_fg)?;
    let bg = HexColor::parse(&pj.color_bg)?;
    let accent = HexColor::parse(&pj.color_accent)?;
    Palette::with_min_contrast(fg, bg, accent, pj.contrast_min)
}

fn decode_typography(json: &str) -> Result<Typography, ThemeableUiError> {
    #[derive(serde::Deserialize)]
    struct Tj {
        font_family: String,
        base_size_px: u16,
    }
    let tj: Tj = serde_json::from_str(json)
        .map_err(|e| ThemeableUiError::Persistence(format!("typography json: {e}")))?;
    Typography::new(tj.font_family, tj.base_size_px)
}

fn encode_palette(p: &Palette) -> String {
    serde_json::json!({
        "color_fg": p.color_fg().as_hex(),
        "color_bg": p.color_bg().as_hex(),
        "color_accent": p.color_accent().as_hex(),
        "contrast_min": p.contrast_min(),
    })
    .to_string()
}

fn encode_typography(t: &Typography) -> String {
    serde_json::json!({
        "font_family": t.font_family(),
        "base_size_px": t.base_size_px(),
    })
    .to_string()
}

#[async_trait]
impl ThemeableUiRepository for SqliteThemeableUiRepository {
    async fn upsert_override(&self, override_: &ThemeOverride) -> Result<(), ThemeableUiError> {
        sqlx::query(
            "INSERT OR REPLACE INTO theme_overrides
             (owner_id, brand_name, logo_path, palette_json, typography_json,
              panel_domain_fqdn, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(override_.owner_id().to_string())
        .bind(override_.brand_name())
        .bind(override_.logo_path())
        .bind(encode_palette(override_.palette()))
        .bind(encode_typography(override_.typography()))
        .bind(override_.panel_domain().map(|d| d.fqdn().to_string()))
        .bind(override_.updated_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| ThemeableUiError::Persistence(format!("upsert override: {e}")))?;
        Ok(())
    }

    async fn find_override(
        &self,
        owner_id: Uuid,
    ) -> Result<Option<ThemeOverride>, ThemeableUiError> {
        let row = sqlx::query(
            "SELECT owner_id, brand_name, logo_path, palette_json, typography_json,
                    panel_domain_fqdn, updated_at
             FROM theme_overrides WHERE owner_id = ?1",
        )
        .bind(owner_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ThemeableUiError::Persistence(format!("find override: {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let owner_id: String = row.get("owner_id");
        let brand_name: String = row.get("brand_name");
        let logo_path: Option<String> = row.get("logo_path");
        let palette_json: String = row.get("palette_json");
        let typography_json: String = row.get("typography_json");
        let panel_fqdn: Option<String> = row.get("panel_domain_fqdn");
        let updated_at: String = row.get("updated_at");
        let owner_id = Uuid::parse_str(&owner_id)
            .map_err(|e| ThemeableUiError::Persistence(format!("owner: {e}")))?;
        let palette = decode_palette(&palette_json)?;
        let typography = decode_typography(&typography_json)?;
        let panel_domain = match panel_fqdn {
            Some(fqdn) => Some(PanelDomain::new(fqdn)?),
            None => None,
        };
        let mut override_ =
            ThemeOverride::new(owner_id, brand_name, palette, typography, panel_domain)?;
        if let Some(p) = logo_path {
            override_.set_logo_path(p);
        }
        let _ = parse_ts(&updated_at); // updated_at is owned by the repo
        Ok(Some(override_))
    }

    async fn delete_override(&self, owner_id: Uuid) -> Result<(), ThemeableUiError> {
        sqlx::query("DELETE FROM theme_overrides WHERE owner_id = ?1")
            .bind(owner_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| ThemeableUiError::Persistence(format!("delete: {e}")))?;
        Ok(())
    }

    async fn find_by_fqdn(&self, fqdn: &str) -> Result<Option<ThemeOverride>, ThemeableUiError> {
        let row = sqlx::query("SELECT owner_id FROM theme_overrides WHERE panel_domain_fqdn = ?1")
            .bind(fqdn)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ThemeableUiError::Persistence(format!("find by fqdn: {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let owner_id: String = row.get("owner_id");
        let owner_id = Uuid::parse_str(&owner_id)
            .map_err(|e| ThemeableUiError::Persistence(format!("owner: {e}")))?;
        self.find_override(owner_id).await
    }
}
