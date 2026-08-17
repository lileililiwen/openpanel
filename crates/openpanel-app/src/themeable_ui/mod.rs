//! Themeable UI and white-label bounded context.

mod repo;
mod service;

pub use repo::SqliteThemeableUiRepository;
pub use service::{DEFAULT_BRANDING_ROOT, MAX_LOGO_BYTES, ThemeableUiService};
