//! Clippy canary: regression tests that prove the workspace's lint policy
//! actually catches unwrap/expect/panic in production code.
//!
//! See `openspec/changes/add-quality-engineering-infrastructure/tasks.md`
//! § 1.1. These tests build a minimal Cargo workspace in a temp dir with
//! the same `[workspace.lints.clippy]` deny rules and a fixture crate
//! that either contains or omits `.unwrap()`, then invokes
//! `cargo clippy --all-targets -- -D warnings` and checks the exit code.
//!
//! Disabled in environments where `cargo` is not on PATH (the assertion
//! would be meaningless and the test would flake on CI images without
//! the toolchain).

#![cfg(test)]

use std::{fs, path::Path, process::Command};

fn cargo_available() -> bool {
    Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write a minimal workspace + member crate into `root` that mirrors the
/// real workspace's `[workspace.lints.clippy]` deny policy. `with_unwrap`
/// controls whether the member's `src/lib.rs` contains a `.unwrap()` on
/// a non-test function.
fn write_fixture(root: &Path, with_unwrap: bool) {
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
        r#"pub fn bad() -> Option<i32> {
    Some(1).map(|x| x + 1).and_then(|x| Some(x))
}

pub fn call_bad() -> i32 {
    // production code — no #[cfg(test)], no allow. The lint must fire.
    let _ = bad();
    Some(2).unwrap()
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
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .expect("failed to spawn cargo clippy")
}

/// Canary: a fresh fixture crate WITH `.unwrap()` in production code MUST
/// fail clippy. If this ever passes, the policy has been silently
/// disabled and the whole quality gate is a lie.
#[test]
fn clippy_catches_unwrap_in_production_code() {
    if !cargo_available() {
        eprintln!("cargo not on PATH; skipping clippy canary");
        return;
    }

    let tmp = tempfile::tempdir().expect("create tempdir");
    write_fixture(tmp.path(), true);

    let status = run_clippy(tmp.path());
    assert!(
        !status.success(),
        "expected clippy to fail on a fixture with `.unwrap()`, but it exited {:?}",
        status.code()
    );
}

/// Counter-test: the same fixture WITHOUT `.unwrap()` MUST pass clippy.
/// Guards against the false positive where any clippy invocation fails
/// (e.g. a broken toolchain would mask the real assertion).
#[test]
fn clippy_passes_on_clean_fixture() {
    if !cargo_available() {
        eprintln!("cargo not on PATH; skipping clippy canary");
        return;
    }

    let tmp = tempfile::tempdir().expect("create tempdir");
    write_fixture(tmp.path(), false);

    let status = run_clippy(tmp.path());
    assert!(
        status.success(),
        "expected clippy to pass on a clean fixture, but it exited {:?}",
        status.code()
    );
}
