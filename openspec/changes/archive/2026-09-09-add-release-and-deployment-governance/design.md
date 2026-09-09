# Design: Release and deployment governance

## Approach

A release is a small, testable directory: a binary, a checksum, an SBOM,
provenance metadata, and a detached signature. A new shell gate
(`scripts/check-release-governance.sh`, wired into `make check` via
`make release-governance`) verifies that every release artifact ships
with the four integrity sidecars before it can be advertised as
"supported". The release matrix is `.github/workflows/release.yml`
(glibc, musl, ARM); the container is `Dockerfile` (non-root, `/health`,
graceful SIGTERM); the installer is `packages/installer/install.sh`
(upgrade + automatic rollback, refuses unsupported downgrades);
schema compatibility is enforced by `openpanel-core::release_metadata`
plus `openpanel-app::release_preflight` (refuses to mutate a store
whose schema is newer than the binary supports).

## Explore & Reuse

- Reuse `.github/workflows/ci.yml` (check, fmt, clippy, test, agent-quality
  jobs) as the workflow style; the new `release.yml` follows the same
  `actions/checkout@v4` + `dtolnay/rust-toolchain@stable` +
  `Swatinem/rust-cache@v2` triad.
- Reuse the `/health` route in `crates/openpanel-api/src/router.rs:194`
  as the container's HEALTHCHECK probe and the rollout readiness gate.
- Reuse `openpanel-core::migration::MigrationRunner`
  (`crates/openpanel-core/src/migration.rs:37`) and the `_migrations`
  ledger it owns; the new preflight reads `applied_versions` to detect a
  too-new schema.
- Reuse `packages/openpanel-menu/scripts/install.sh` as the OS-detection
  pattern (alpine / debian / rhel / arch); the new
  `packages/installer/install.sh` adds the upgrade / rollback subcommands
  on the same distribution matrix.
- Reuse `scripts/lib/step.sh` (`step: <name> status: ok | failed`)
  and the existing `make check` chain — the new gate plugs in the same
  way `file-length`, `scan-literal`, and `class-coverage` do.
- Reuse `CARGO_PKG_VERSION` (already embedded by
  `crates/openpanel-cli/src/handlers.rs:103`,
  `crates/openpanel-web/src/settings.rs:104`) as the
  `BINARY_VERSION` constant; add `BUILD_COMMIT` and `BUILD_TARGET`
  via `build.rs`-set `OPENPANEL__BUILD__*` env! overrides
  (mirrors the `OPENPANEL__DATABASE__MASTER_KEY` env-var pattern in
  `Agents.md` §7).
- Reuse `cargo metadata --format-version=1 --no-deps` for SBOM
  generation (no network, deterministic, ships in `rustup`).
- Reuse `minisign` (optional, falls back to a fingerprint-only check
  when the tool is absent) for detached signatures; the public key
  fingerprint is published in `docs/RELEASE.md`.

## Boundaries

- The release workflow builds and attests artifacts; the installer
  handles local upgrade/rollback on the host; the application owns
  schema-version compatibility. A CI job never mutates a production
  host and never embeds signing secrets.
- The new gate is **read-only**: it inspects a `dist/` directory
  produced by `release.yml` and reports missing sidecars; it MUST
  NOT rewrite artifacts, push to a registry, or sign in place.
- The signature verification is **fail-closed**: with
  `RELEASE_VERIFY_KEY` unset, the gate accepts a self-signed
  fingerprint placeholder (this is the test-mode behaviour). With the
  env var set to a real minisign public key, the gate refuses any
  artifact whose `.sig` does not verify.

## Deliverables (concrete file list)

| File | Purpose |
|---|---|
| `.github/workflows/release.yml` | Release matrix: glibc, musl, ARM; builds + packages + signs + uploads; runs `make check` first |
| `Dockerfile` | Distroless-style minimal container, non-root, `/health` HEALTHCHECK, `STOPSIGNAL SIGTERM` |
| `.dockerignore` | Excludes `target/`, `.git/`, `node_modules/`, etc. |
| `packages/installer/install.sh` | OS-detect (reuses the menu installer's distribution matrix); `install`, `upgrade`, `rollback` subcommands; refuses unsupported downgrade |
| `packages/installer/install-tests.sh` | Black-box install / upgrade / rollback / unsupported-OS fixtures |
| `packages/installer/smoke-container.sh` | Container smoke: `/health`, `/metrics`, non-root UID, graceful SIGTERM |
| `crates/openpanel-core/src/release_metadata.rs` | Compile-time `BinaryMetadata { version, commit, target, max_schema_version, build_timestamp }` |
| `crates/openpanel-core/src/release_metadata/tests.rs` | Unit tests for `BinaryMetadata::current` + `max_schema_version` ordering |
| `crates/openpanel-app/src/release_preflight.rs` | `PreflightOutcome::TooNewSchema { applied, supported_max }`; pure decision function + integration with `MigrationRunner::applied_versions` |
| `crates/openpanel-app/src/release_preflight/tests.rs` | Property + integration tests for the preflight decision |
| `crates/openpanel-core/build.rs` | Captures `VERGEN_GIT_SHA` / `VERGEN_BUILD_TIMESTAMP` from env (fails open when unset) |
| `crates/openpanel-core/src/lib.rs` | Re-exports `release_metadata` |
| `crates/openpanel-app/src/lib.rs` | Re-exports `release_preflight` |
| `scripts/check-release-governance.sh` | New gate: walks `dist/`, asserts each binary has `.sha256` + `.sbom.json` + `.sig` + provenance root; honours `RELEASE_VERIFY_KEY` |
| `scripts/lib/release-fixture.sh` | Helper that builds a passing or failing `dist/` fixture for the self-test |
| `scripts/test-gates.sh` | New positive + negative fixtures for `release-governance` (`# checker: release-governance positive|negative`) |
| `Makefile` | New `release-governance` target; `check` target gains `release-governance` between `class-coverage` and `tasks-testing-first` |
| `docs/RELEASE.md` | Supported-platform matrix, signing key fingerprint, upgrade/rollback runbook, reproducibility recipe (`SOURCE_DATE_EPOCH`, `cargo build --release` flags) |
| `openspec/specs/quality/spec.md` (delta) | One new requirement: `Release Governance Gate` — `make check` runs `release-governance`; missing / unsigned artifact fails the build |

## Verification

- **Unit**: `crates/openpanel-core/src/release_metadata/tests.rs` covers
  `BinaryMetadata::current`, `max_schema_version` monotonicity, and the
  build-env fallback path.
- **Unit / property**: `crates/openpanel-app/src/release_preflight/tests.rs`
  covers `PreflightOutcome` for `{fresh, current, ahead_by_one, far_ahead,
  empty_ledger}` and asserts the decision is stable under `proptest`.
- **Integration (install)**: `packages/installer/install-tests.sh` runs in
  disposable rootfs containers (Docker) for `debian:bookworm-slim`,
  `fedora:40`, `alpine:3.20`, `archlinux:latest` — clean install,
  upgrade-over-existing, rollback-after-failed-upgrade, unsupported-OS
  exit 78.
- **Integration (container)**: `packages/installer/smoke-container.sh`
  builds the Dockerfile, runs the container with a tmpfs data volume,
  asserts `/health` returns 200, asserts `id -u` is non-zero, sends
  `SIGTERM` and asserts clean exit.
- **Workflow lint**: `scripts/test-gates.sh` registers a
  `release-governance` checker with positive (`dist/` with all sidecars)
  and negative (`dist/` missing `.sbom.json` OR unsigned OR
  unreferenced-by-provenance) fixtures.
- **Reproducibility (manual + CI)**: `docs/RELEASE.md` documents the
  exact recipe (`SOURCE_DATE_EPOCH=<commit-time> cargo build --release
  --locked --target <target>`); a CI job `release.yml` runs the recipe
  twice on the same commit and asserts the SHA-256 of the stripped
  binary matches.

## Non-goals

- Multi-region orchestration.
- Automatic database rollback after an irreversible migration.
- Vendor-specific cloud deployment.
- Online signature key infrastructure (HSM, KMS) — the minisign
  public-key model is sufficient for v1.
- Migration from a too-old to a too-new schema in a single upgrade
  (the operator MUST run the intermediate version; the preflight
  surfaces the missing step).
