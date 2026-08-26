//! Per-resource HTTP route modules. Each submodule exposes a `router(...)` builder.

/// Account hierarchy routes (`/users/{id}/children`, `/users/{id}/tree`).
pub mod account_hierarchy;
pub mod api_tokens;
pub mod app_runtimes;
/// Backup and restore routes (`/backups`).
pub mod backups;
/// Per-site collaborator routes (`/sites/{id}/collaborators`).
pub mod collaborators;
/// Container registry routes (`/registry`).
pub mod container_registry;
/// Container runtime routes (`/container`).
pub mod container_runtime;
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
/// Hosting plan routes (`/hosting-plans`).
pub mod hosting_plans;
/// Identity resource routes (`/identity`).
pub mod identity;
/// Authorized log and traffic routes (`/logs`).
pub mod logs;
/// Hosted mail routes (`/mail`).
pub mod mail;
/// Malware scanner routes (`/sites/{id}/scan`, `/scans/{id}`).
pub mod malware_scanner;
/// Monitoring resource routes (`/monitoring`).
pub mod monitoring;
pub mod notifications;
/// Plugin extension framework routes (`/plugins`).
pub mod plugin_extension;
/// Plugin marketplace discovery routes (`/marketplace`).
pub mod plugin_marketplace;
/// Host firewall and login-abuse routes (`/security`).
pub mod security;
/// SSL resource routes (`/ssl`).
pub mod server_snapshots;
/// Site cache policy and CDN integration routes
/// (`/sites/{id}/cache`, `/cdn/integrations`, `/cdn/purge`).
pub mod site_cache_cdn;
/// Site clone + template export routes
/// (`/sites/{id}/clone`, `/sites/{id}/export-template`, `/sites/templates`).
pub mod site_clone_template;
/// Per-site web application firewall routes (`/sites/{id}/waf`).
pub mod site_http_controls;
/// Per-site staging routes (`/sites/{id}/staging/...`).
pub mod site_staging;
/// Sites resource routes (`/sites`).
pub mod sites;
/// Curated Software Center routes (`/software`).
pub mod software_center;
pub mod ssl;
pub mod sso;
/// Allowlisted system-service routes (`/services`).
pub mod system_services;
/// Themeable UI / white-label routes (`/admin/branding`).
pub mod themeable_ui;
pub mod waf;
/// Web application installer routes
/// (`/sites/{id}/web-apps`, `/web-apps/{id}`).
pub mod web_application_installer;
pub mod web_terminal;
