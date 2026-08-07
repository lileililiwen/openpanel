//! Test fixtures, helpers, and mocks for the OpenPanel workspace.
//!
//! This crate is `dev-dependencies` only — it MUST NOT appear in
//! `[dependencies]` of any production crate.

#![deny(rustdoc::broken_intra_doc_links)]

pub mod db;
pub mod mocks;
pub mod server;

pub use db::TestDb;
pub use server::TestServer;
