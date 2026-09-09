# quality Specification (delta)

## ADDED Requirements

### Requirement: Release Governance Gate

`make check` SHALL run a new `release-governance` step
(`scripts/check-release-governance.sh`). The gate MUST walk every
artifact under the directory named by `OPENPANEL_DIST_DIR`
(default `dist/`) and fail the build when, for any artifact, the
required integrity sidecars are missing or inconsistent. The required
sidecars per binary are:

- `<artifact>.sha256` — the SHA-256 of the binary in hex.
- `<artifact>.sbom.json` — a CycloneDX-style SBOM derived from
  `cargo metadata` (no network call required; the generator is
  deterministic and ships in the repo).
- `<artifact>.sig` — a detached signature. With
  `RELEASE_VERIFY_KEY` set, the gate MUST verify the signature with
  minisign and exit non-zero on any mismatch. With the env var unset
  (the test-mode default), the gate accepts a self-signed fingerprint
  placeholder and reports `step: release-governance status: skipped
  (RELEASE_VERIFY_KEY unset)` only when the sidecars themselves are
  complete; missing sidecars still fail.
- `provenance.json` at the dist root — MUST list every artifact, the
  build commit, the build target, and the `SOURCE_DATE_EPOCH` used
  during the build; any artifact not listed in `provenance.json` is
  treated as unreleased.

The gate MUST exit zero when `OPENPANEL_DIST_DIR` does not exist
(local development) so `make check` does not block unrelated work,
and MUST print `step: release-governance status: skipped (no
OPENPANEL_DIST_DIR)` so the skip is auditable.

The gate MUST be read-only: it MUST NOT modify any artifact, push to
any registry, or sign anything in place. A broken signature check
MUST surface a non-zero exit before any publication step runs.

#### Scenario: Complete release passes

- **WHEN** `dist/` contains `openpanel`, `openpanel.sha256`,
  `openpanel.sbom.json`, `openpanel.sig`, and a `provenance.json`
  that lists `openpanel` with the matching build commit and target
- **THEN** `make release-governance` exits zero and prints
  `step: release-governance status: ok`.

#### Scenario: Missing SBOM fails

- **WHEN** `dist/openpanel` exists but `dist/openpanel.sbom.json`
  does not
- **THEN** `make release-governance` exits non-zero and names
  `openpanel` and the missing sidecar.

#### Scenario: Unlisted artifact fails

- **WHEN** `dist/openpanel-agent` exists and is signed and checksumed
  but `provenance.json` does not list it
- **THEN** `make release-governance` exits non-zero and reports
  `provenance: artifact openpanel-agent not listed`.

#### Scenario: Signature mismatch with verify key

- **WHEN** `RELEASE_VERIFY_KEY` is set and `dist/openpanel.sig`
  does not verify against the public key
- **THEN** `make release-governance` exits non-zero and reports the
  failing artifact.

#### Scenario: No dist directory skips

- **WHEN** `OPENPANEL_DIST_DIR` does not exist
- **THEN** `make release-governance` prints
  `step: release-governance status: skipped (no OPENPANEL_DIST_DIR)`
  and exits zero.
