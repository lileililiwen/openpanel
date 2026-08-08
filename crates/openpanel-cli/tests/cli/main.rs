//! CLI E2E tests — single binary so the shared helper compiles once and
//! every field/method is genuinely used (no `#[allow(dead_code)]`).

// Workspace lints deny `unwrap_used` / `expect_used` / `panic` in
// production code. E2E tests are test code and MAY contain them, so we
// allow them here for this test binary.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod database;
mod file;
mod serve;
mod site;
mod ssl;
mod user;
