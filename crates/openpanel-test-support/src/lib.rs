//! Test fixtures, helpers, and mocks for the OpenPanel workspace.
//!
//! This crate is `dev-dependencies` only — it MUST NOT appear in
//! `[dependencies]` of any production crate.

#![deny(rustdoc::broken_intra_doc_links)]
// This crate exists solely to provide test infrastructure. `expect()` /
// `unwrap()` / `panic!` are legitimate here: setup code that fails fast
// if the test environment is broken is the correct behavior. The lints
// remain enforced for everything else via `[lints] workspace = true`.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[cfg(test)]
mod clippy_canary;
pub mod db;
pub mod mocks;
pub mod server;

pub use db::TestDb;
pub use hosting_plans::MemoryHostingPlanRepository;
pub use mocks::{
    MockAudit, MockDatabaseRepo, MockFactorRepo, MockSessionRepo, MockSiteRepo, MockSnapshotRepo,
    MockUserRepo,
};
pub use server::TestServer;

mod hosting_plans;
