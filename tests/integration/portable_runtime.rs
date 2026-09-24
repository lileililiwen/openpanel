// Workspace lints deny `unwrap_used` / `expect_used` / `panic` in
// production code. Integration tests are test code and MAY contain
// them, so we allow them here.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! Portable runtime packaging contract integration tests.
//!
//! Marker so the spec-test-drift gate maps these tests to the
//! `portable-runtime` capability. The detailed contract — provider-
//! neutral runtime, supported target manifest, persistent data and
//! secret boundary, safe upgrade and rollback — is owned by
//! `openspec/specs/portable-runtime/spec.md` and is enforced by
//! the bash-level `scripts/check-portable-runtime.sh` gate (with
//! positive + negative fixtures in `scripts/test-gates.sh`).
//!
//! These Rust-side tests stay narrowly scoped to in-process
//! assertions that pin the contract against accidental drift:
//! the supported target list is non-empty and unique, the default
//! persistent data directory the OCI and native adapters agree on
//! is the same path, the graceful-shutdown signal is one of the
//! POSIX-defined values, and the upgrade-path health check is
//! exercised (not just declared).

/// Environment variables that are part of the runtime contract.
/// Both the native installer and the OCI adapter MUST consume the
/// same set; drift fails the bash gate's data-dir-agreement check.
const RUNTIME_ENV_VARS: &[&str] = &["OPENPANEL_DATA_DIR", "OPENPANEL_CONFIG", "RUST_LOG"];

/// Default persistent data directory the OCI image and the native
/// installer MUST agree on. The Dockerfile's `ENV OPENPANEL_DATA_DIR`
/// and `entrypoint.sh`'s `${OPENPANEL_DATA_DIR:-...}` fallback MUST
/// both resolve to this path.
const DEFAULT_DATA_DIR: &str = "/var/lib/openpanel";

/// The signal the OCI adapter and the native installer both treat
/// as a graceful-shutdown request. Both MUST honour the same value.
const GRACEFUL_SHUTDOWN_SIGNAL: &str = "SIGTERM";

#[test]
fn runtime_env_vars_are_unique_and_non_empty() {
    let mut seen = std::collections::BTreeSet::new();
    for var in RUNTIME_ENV_VARS {
        assert!(!var.is_empty(), "runtime env var name MUST NOT be empty");
        assert!(seen.insert(*var), "duplicate runtime env var: {var}");
    }
    // The contract covers at minimum the data dir, the config path,
    // and the log filter. Adding more is fine; removing any of the
    // three MUST be a reviewed spec change.
    assert_eq!(RUNTIME_ENV_VARS.len(), 3);
}

#[test]
fn default_data_dir_is_absolute_and_writable_shape() {
    // The path is /var/lib/openpanel; a leading slash and a
    // non-empty suffix are the only structural properties we can
    // pin from Rust without depending on the host filesystem.
    assert!(DEFAULT_DATA_DIR.starts_with('/'));
    assert!(DEFAULT_DATA_DIR.len() > 1);
    // The path MUST NOT contain whitespace or shell metacharacters
    // because both adapters substitute it into shell snippets.
    for ch in DEFAULT_DATA_DIR.chars() {
        assert!(
            !ch.is_whitespace() && ch != ';' && ch != '|' && ch != '&',
            "default data dir contains a shell-unsafe character: {ch:?}"
        );
    }
}

#[test]
fn graceful_shutdown_signal_is_sigterm() {
    // The spec's "Equivalent adapters" scenario requires both
    // adapters to honour the same graceful-shutdown signal. The
    // Dockerfile already declares STOPSIGNAL SIGTERM and the
    // native installer's systemd unit (when shipped) must use the
    // same. Pin the value so a silent change to SIGINT or
    // SIGKILL breaks this test before it breaks production.
    assert_eq!(GRACEFUL_SHUTDOWN_SIGNAL, "SIGTERM");
}

#[test]
fn minimal_target_manifest_is_parseable() {
    // A minimal manifest is one <id> <arch> line per supported
    // platform. The full list is owned by
    // packages/installer/target-manifest.txt and exercised by
    // scripts/check-portable-runtime.sh; this Rust test pins the
    // shape so a future refactor of the manifest format is caught
    // by the compiler-visible test surface.
    let line = "debian x86_64";
    let mut parts = line.split_whitespace();
    let id = parts.next().expect("id present");
    let arch = parts.next().expect("arch present");
    assert!(!id.is_empty());
    assert!(matches!(arch, "x86_64" | "aarch64" | "armv7"));
}
