//! Importer drivers for the migration-importers bounded context.
//!
//! The `TarWithJsonManifestDriver` implements the panel's own
//! deterministic backup format: a gzip/tar bundle carrying a single
//! `manifest.json` entry that describes every resource. It is the
//! reference driver for preview / run / rollback and the format the
//! panel itself can emit for hand-crafted imports and test fixtures.
//!
//! The cPanel `pkgacct`, `cpanel-legacy-backup`, and Baota
//! `/www/backup/` drivers share the same `MigrationDriver` contract
//! but are parsed from their native tar layouts; they ship in the
//! follow-on change once the bundle fixtures are in place.

pub mod tar_json_manifest;

pub use tar_json_manifest::{
    JsonManifest, JsonManifestBundle, ManifestResource, ManifestResourceExt, ManifestTranslator,
    RefuseAll, TarWithJsonManifestDriver, parse_manifest, sniff_tar_manifest,
};
