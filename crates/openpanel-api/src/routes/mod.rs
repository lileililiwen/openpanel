//! Per-resource HTTP route modules. Each submodule exposes a `router(...)` builder.

/// Backup and restore routes (`/backups`).
pub mod backups;
/// Cron scheduling routes (`/cron`).
pub mod cron;
/// Databases resource routes (`/databases`).
pub mod databases;
/// Provider-backed DNS routes (`/dns`).
pub mod dns;
/// Files resource routes (`/files/{site_id}/...`).
pub mod files;
/// Identity resource routes (`/identity`).
pub mod identity;
/// Authorized log and traffic routes (`/logs`).
pub mod logs;
/// Hosted mail routes (`/mail`).
pub mod mail;
/// Monitoring resource routes (`/monitoring`).
pub mod monitoring;
/// Host firewall and login-abuse routes (`/security`).
pub mod security;
/// Sites resource routes (`/sites`).
pub mod sites;
/// Curated Software Center routes (`/software`).
pub mod software_center;
/// SSL resource routes (`/ssl`).
pub mod ssl;
/// Allowlisted system-service routes (`/services`).
pub mod system_services;
