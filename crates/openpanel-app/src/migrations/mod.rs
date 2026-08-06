//! Embedded SQL migrations. Each module places its migration files under
//! `migrations/<module_name>/V###__*.sql`. The `apply` helper in core walks
//! the directory at runtime.

pub const AUDIT_V001: &str = include_str!("000_audit.sql");
pub const IDENTITY_V001: &str = include_str!("identity/V001__init.sql");
pub const SITES_V001: &str = include_str!("sites/V001__init.sql");