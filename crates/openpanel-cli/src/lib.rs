//! CLI commands. Re-exported by `lib.rs` for use by the binary entry point.

#![deny(rustdoc::broken_intra_doc_links)]

/// Clap-derived CLI command tree (top-level `Command` enum and subcommand enums).
pub mod commands;
/// CLI command handlers — each delegates to the corresponding service in `openpanel-app`.
pub mod handlers;

/// Re-export of the full CLI command tree so the binary entry point can import it directly.
pub use commands::*;
