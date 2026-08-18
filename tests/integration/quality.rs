//! End-to-end quality gate tests.
//!
//! See `openspec/changes/add-quality-engineering-infrastructure/tasks.md`
//! § 1.2.
//!
//! These tests prove the project's `make check` gate is effective:
//!   1. The repo itself (treated as the "fresh fixture workspace" the gate
//!      runs against) exits 0 from `make check`.
//!   2. A minimal fixture workspace whose `src/lib.rs` contains
//!      `.unwrap()` in production code FAILS the same clippy gate — i.e.
//!      a regression in the policy is caught immediately rather than at
//!      code review.
//!
//! Tests gracefully skip when `cargo` is not on PATH so CI images without
//! a Rust toolchain still report a green suite instead of a flake.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn cargo_available() -> bool {
    Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn make_available() -> bool {
    Command::new("make")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve the workspace root via `CARGO_MANIFEST_DIR`, which cargo sets
/// to the package root for every test invocation.
fn repo_root() -> PathBuf {
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"))
}

/// Run the quality gates against the repo. We deliberately invoke the
/// four non-test gates (`fmt clippy docs audit`) instead of the full
/// `make check`, because `make check` includes `make test`, which
/// would re-run this very test binary and recurse.
///
/// If these four gates pass against the current codebase, the gate
/// stack is wired correctly and `make check` (run by humans / CI) will
/// also pass.
#[test]
fn non_test_gates_pass_on_repo() {
    // When `make check` invokes this suite (via scripts/check-tests.sh),
    // the non-test gates already ran in the parent process. Re-running a
    // full `cargo clippy` + `cargo doc` compile here would duplicate
    // minutes of work and starve every other test of CPU under parallel
    // execution, so the parent sets OPENPANEL_TEST_SKIP_GATE_CANARY.
    // Standalone `cargo test` (no env var) still runs the canary.
    if std::env::var_os("OPENPANEL_TEST_SKIP_GATE_CANARY").is_some() {
        return;
    }
    if !cargo_available() || !make_available() {
        eprintln!("cargo/make not on PATH; skipping gate canary");
        return;
    }
    let root = repo_root();

    let status = Command::new("make")
        .current_dir(&root)
        .args(["fmt", "clippy", "docs", "audit"])
        .status()
        .expect("failed to spawn `make fmt clippy docs audit`");

    assert!(
        status.success(),
        "non-test quality gates exited with {:?}; the gate stack is broken",
        status.code()
    );
}

/// Build a minimal workspace in `root` whose `[workspace.lints.clippy]`
/// mirrors the real repo's deny policy. `with_unwrap` controls whether
/// the member crate's `src/lib.rs` contains a `.unwrap()` call outside
/// of `#[cfg(test)]`.
fn write_workspace(root: &Path, with_unwrap: bool) {
    fs::create_dir_all(root.join("crates/fixture/src")).unwrap();

    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["crates/fixture"]

[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
"#,
    )
    .unwrap();

    fs::write(
        root.join("crates/fixture/Cargo.toml"),
        r#"[package]
name = "fixture"
version = "0.0.0"
edition = "2024"

[lints]
workspace = true
"#,
    )
    .unwrap();

    let body = if with_unwrap {
        r#"pub fn good(x: Option<i32>) -> i32 {
    x.map_or(0, |n| n + 1)
}

pub fn call_bad() -> i32 {
    // production code: workspace unwrap_used=deny must reject this.
    Some(1).unwrap()
}
"#
    } else {
        r#"pub fn good(x: Option<i32>) -> i32 {
    x.map_or(0, |n| n + 1)
}
"#
    };
    fs::write(root.join("crates/fixture/src/lib.rs"), body).unwrap();
}

fn run_clippy(root: &Path) -> std::process::ExitStatus {
    Command::new("cargo")
        .current_dir(root)
        .args(["clippy", "--all-targets", "--", "-D", "warnings"])
        .status()
        .expect("failed to spawn cargo clippy")
}

/// A workspace with NO `.unwrap()` passes the clippy gate.
#[test]
fn clean_fixture_passes_clippy() {
    if !cargo_available() {
        eprintln!("cargo not on PATH; skipping fixture gate canary");
        return;
    }
    let tmp = tempfile::tempdir().expect("create tempdir");
    let _path: PathBuf = tmp.path().to_path_buf();
    write_workspace(tmp.path(), false);

    let status = run_clippy(tmp.path());
    assert!(
        status.success(),
        "clean fixture unexpectedly failed clippy (exit {:?})",
        status.code()
    );
}

/// A workspace WITH `.unwrap()` in production code fails the clippy
/// gate. This is the regression assertion for task 11.6.
#[test]
fn fixture_with_unwrap_fails_clippy() {
    if !cargo_available() {
        eprintln!("cargo not on PATH; skipping fixture gate canary");
        return;
    }
    let tmp = tempfile::tempdir().expect("create tempdir");
    write_workspace(tmp.path(), true);

    let status = run_clippy(tmp.path());
    assert!(
        !status.success(),
        "fixture with `.unwrap()` unexpectedly passed clippy (exit {:?}) — \
         the unwrap_used=deny policy is not enforced",
        status.code()
    );
}
