//! Per-resource HTTP route modules. Each submodule exposes a `router(...)` builder.

pub mod api_tokens;
/// Backup and restore routes (`/backups`).
pub mod backups;
/// Cron scheduling routes (`/cron`).
pub mod cron;
/// Databases resource routes (`/databases`).
pub mod databases;
/// Database point-in-time recovery routes
/// (`/backups/databases/{id}/pitr/...`).
pub mod db_pitr;
/// Provider-backed DNS routes (`/dns`).
pub mod dns;
pub mod docker;
/// Files resource routes (`/files/{site_id}/...`).
pub mod files;
/// Per-site FTP account routes (`/sites/{id}/ftp/accounts`).
pub mod ftp;
/// Identity resource routes (`/identity`).
pub mod identity;
/// Authorized log and traffic routes (`/logs`).
pub mod logs;
/// Hosted mail routes (`/mail`).
pub mod mail;
/// Monitoring resource routes (`/monitoring`).
pub mod monitoring;
pub mod notifications;
/// Plugin marketplace discovery routes (`/marketplace`).
pub mod plugin_marketplace;
/// Host firewall and login-abuse routes (`/security`).
pub mod security;
/// Per-site staging routes (`/sites/{id}/staging/...`).
pub mod site_staging;
/// Sites resource routes (`/sites`).
pub mod sites;
/// Curated Software Center routes (`/software`).
pub mod software_center;
/// SSL resource routes (`/ssl`).
pub mod ssl;
/// Allowlisted system-service routes (`/services`).
pub mod system_services;
/// Per-site web application firewall routes (`/sites/{id}/waf`).
pub mod waf;
