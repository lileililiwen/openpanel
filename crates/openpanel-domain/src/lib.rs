//! Domain layer: entities, value objects, aggregates, repository traits.
//! Zero I/O — no sqlx, no axum, no tokio. Implementation lives in
//! `openpanel-app`.

#![deny(rustdoc::broken_intra_doc_links)]
// Workspace lints deny `unwrap_used` / `expect_used` / `panic` in
// production. Test code (`#[cfg(test)]`) MAY contain these, so we relax
// the deny to a warn for test compilation only.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// Backup plans, manifests, runs, and safe restore values.
/// Scoped personal API token credentials and lifecycle invariants.
pub mod api_tokens;
pub mod backups;
pub mod common;
/// Scheduled command and HTTP job domain model.
pub mod cron;
pub mod databases;
/// Provider-backed DNS zones and typed records.
pub mod dns;
pub mod docker;
pub mod files;
/// Per-site FTP accounts, limits, and chroot path policy.
pub mod ftp;
pub mod identity;
pub mod logs;
/// Hosted mail domains, addresses, quotas, aliases, and relay policy.
pub mod mail;
pub mod monitoring;
/// Notification channels, subscriptions, events, and durable delivery state.
pub mod notifications;
pub mod security;
pub mod sites;
/// Trusted software catalog, transaction plans, and job lifecycle invariants.
pub mod software_center;
pub mod ssl;
/// Allowlisted host service lifecycle and health domain.
pub mod system_services;
/// Per-site typed web application firewall rules.
pub mod waf;
/// Plugin extension framework: signed manifests, capability gating,
/// lifecycle, and supervisor-facing repository trait.
pub mod plugin;
/// Plugin marketplace: remote catalog discovery, publisher CA
/// verification, and rating/metadata cache.
pub mod plugin_marketplace;

/// Database point-in-time recovery: continuous binlog streaming,
/// point-in-time restore, and incremental file-backup deltas.
pub mod db_pitr;
/// Per-site staging slots, sync policies, and atomic promote.
pub mod site_staging;

/// Internationalization: locale, catalog, locale negotiation, formatter.
pub mod i18n;
/// Observability export: Prometheus metrics, OTLP traces, JSONL logs.
pub mod observability_export;

pub use api_tokens::{
    ApiToken, ApiTokenError, ApiTokenMetadata, ApiTokenRepository, Cidr, TokenCredential,
    TokenHash, TokenScope,
};
pub use common::{
    Email, Password, PasswordError, Username, UsernameError,
    error::{DomainError, RepoError},
};
pub use databases::{
    database::Database, engine::DatabaseEngine, error::DatabaseError,
    repository::DatabaseRepository, status::DatabaseStatus,
};
pub use db_pitr::{
    BinlogRange, BinlogSegment, BinlogSink, BinlogStream, BinlogStreamRepository,
    BinlogStreamStatus, DatabaseLookup, IncrementalBackup, IncrementalMode, IncrementalRepository,
    LogSeq, LogTailer, PitrError, PitrRepository, PitrRestore, PitrRestoreStatus, ReplayOutcome,
    RestoreReplayWindow, RestoreTimestamp, StreamTargetId,
};
pub use files::{
    error::FileError,
    file_info::FileInfo,
    path::Path,
    repository::{FileRepository, MAX_READ_BYTES},
};
pub use identity::{
    error::IdentityError,
    remember::{
        REMEMBER_DEVICE_COOKIE, REMEMBER_DEVICE_DEFAULT_LIFETIME, REMEMBER_DEVICE_VERSION,
        RememberedDevicePayload, hash_user_agent, ip_prefix,
    },
    repository::{SessionRepository, UserRepository},
    role::Role,
    session::{Session, SessionBuilder, SessionToken, SessionTokenError},
    user::User,
};
pub use monitoring::{
    Alert, AlertRule, DiskReading, MetricKind, MetricSample, MonitoringError, NetworkReading,
    SnapshotRepository, SystemSnapshot, Unit,
};
pub use plugin::{
    Capability, CapabilitySet, ManifestRuntime, PluginError, PluginId, PluginManifest,
    PluginRecord, PluginRegistry, PluginStatus, PluginVersion, PublisherKey,
};
pub use plugin_marketplace::{
    CatalogCache, CatalogSnapshot, MarketplaceCa, MarketplaceCatalog, MarketplacePlugin,
    PluginMarketplaceError, PluginRating, PublisherSignature, SignedCatalogEnvelope,
    verify_envelope,
};
pub use site_staging::{
    PromotionRepository, PromotionRun, PromotionStatus, SiteStagingError, SnapshotId, StagingSlot,
    StagingSlotRepository, StagingSnapshotRepository, SyncMode, SyncPolicy,
};
pub use sites::{error::SiteError, repository::SiteRepository, site::Site, status::SiteStatus};
pub use ssl::{
    certificate::{Certificate, KeyType},
    error::SslError,
    repository::CertificateRepository,
    source::{CertificateSource, CertificateStatus},
};
pub use waf::{Rule, RuleSet, WafError, WafRepository};
