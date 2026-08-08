//! Per-resource HTTP route modules. Each submodule exposes a `router(...)` builder.

/// Databases resource routes (`/databases`).
pub mod databases;
/// Files resource routes (`/files/{site_id}/...`).
pub mod files;
/// Identity resource routes (`/identity`).
pub mod identity;
/// Monitoring resource routes (`/monitoring`).
pub mod monitoring;
/// Sites resource routes (`/sites`).
pub mod sites;
/// SSL resource routes (`/ssl`).
pub mod ssl;
