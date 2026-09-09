# Release and deployment governance

OpenPanel ships as a single static Rust binary per supported target.
This document is the operator runbook for the release pipeline and
the on-host install/upgrade/rollback workflow. The spec source of
truth is `openspec/specs/release-deployment-governance/spec.md`; the
quality spec for the blocking release gate is
`openspec/specs/quality/spec.md` (`Release Governance Gate`).

## Supported platforms

The release matrix is the same one the CI workflow builds and the
container image targets. Adding a new target is a reviewed change
that updates the spec, the matrix, and this document.

| Target | Triple | Use case |
|---|---|---|
| glibc, x86_64 | `x86_64-unknown-linux-gnu` | Default Linux install (Debian, Ubuntu, Fedora, RHEL, Arch) |
| musl, x86_64 | `x86_64-unknown-linux-musl` | Static binary, Alpine, scratch containers |
| glibc, AArch64 | `aarch64-unknown-linux-gnu` | ARM servers, AWS Graviton, Raspberry Pi OS 64-bit |

## Artifact layout

Every release produces, under `dist/`, a binary per target plus
three sidecars per binary and a single provenance manifest at the
root:

```
dist/
├── openpanel-x86_64-unknown-linux-gnu
├── openpanel-x86_64-unknown-linux-gnu.sha256
├── openpanel-x86_64-unknown-linux-gnu.sbom.json
├── openpanel-x86_64-unknown-linux-gnu.sig
├── openpanel-cli-x86_64-unknown-linux-gnu
├── openpanel-cli-x86_64-unknown-linux-gnu.sha256
├── openpanel-cli-x86_64-unknown-linux-gnu.sbom.json
├── openpanel-cli-x86_64-unknown-linux-gnu.sig
└── provenance.json
```

`scripts/check-release-governance.sh` (wired into `make check` via
`make release-governance`) refuses to advertise a release unless
every binary ships with all three sidecars and is listed in
`provenance.json`. The gate is **read-only** and never modifies an
artifact.

## Signing

`release.yml` signs each artifact with `minisign`. The private key
is held in `RELEASE_SIGNING_KEY` (a CI secret) and is never embedded
in a release artifact or in this repository. With the secret unset,
the workflow writes a placeholder fingerprint and the gate accepts it
in test mode; set the secret to fail-closed on any unsigned artifact.

The public key fingerprint that operators verify against is published
in the GitHub Release body and in the in-binary `/about` response.

## Reproducibility

A reproducible build uses the same Cargo.lock, the same toolchain,
and `SOURCE_DATE_EPOCH` set to the commit timestamp. The release
workflow exports `SOURCE_DATE_EPOCH=${{ github.event.repository.updated_at }}`
and uses `cargo build --release --locked --target <target>` followed
by `strip`. The dry-run job (`workflow_dispatch`) builds twice from a
clean state and asserts the SHA-256 of the stripped binary is
identical; the workflow fails if the two clean builds diverge.

A failed reproducibility check is a release blocker. To reproduce a
release locally:

```sh
export SOURCE_DATE_EPOCH="$(git log -1 --pretty=%ct)"
cargo build --release --locked --target x86_64-unknown-linux-gnu
strip target/x86_64-unknown-linux-gnu/release/openpanel
sha256sum target/x86_64-unknown-linux-gnu/release/openpanel
```

## Container

The official image (`Dockerfile`) is built on top of
`debian:bookworm-slim`, runs as the `openpanel` system user (uid
10001 by default), and uses `tini` as PID 1. Data is persisted
through a single declared volume at `/var/lib/openpanel`.

The container ships the same `/health` endpoint as the on-host
install; the `HEALTHCHECK` directive in the `Dockerfile` calls
`openpanel healthcheck` so a Docker daemon can gate traffic on the
endpoint without baking an HTTP probe into the image. `STOPSIGNAL
SIGTERM` is honored; the entrypoint forwards it to the binary and
the binary drains in-flight requests before exiting.

```sh
docker build -t openpanel:dev .
docker run --rm -p 8080:8080 -v openpanel-data:/var/lib/openpanel openpanel:dev
```

The smoke test (`packages/installer/smoke-container.sh`) is the
contract:

1. `/health` returns `200` within 30 s of `docker run`.
2. `id -u` inside the container is non-zero.
3. `docker kill --signal SIGTERM` stops the container within 10 s.

## On-host install / upgrade / rollback

`packages/installer/install.sh` is the operator entry point. It
auto-detects the distribution, creates the `openpanel` system user,
and refuses to run on anything outside the supported matrix (exit 78,
`EX_CONFIG`).

```sh
# Fresh install
sudo bash packages/installer/install.sh install

# Upgrade a running host (refuses unsupported downgrades)
sudo OPENPANEL_MAX_SCHEMA_VERSION=$(openpanel --version | awk '{print $NF}') \
     bash packages/installer/install.sh upgrade --from ./openpanel

# Roll back to the most recent binary backup
sudo bash packages/installer/install.sh rollback
```

`upgrade` performs three preflight checks before swapping bytes:

1. The new binary exists and is readable.
2. The existing `${OPENPANEL_DATA_DIR}/openpanel.sqlite` database
   contains a `_migrations` table whose highest applied version is
   `<= OPENPANEL_MAX_SCHEMA_VERSION`. A too-new schema blocks the
   upgrade with an actionable error.
3. The existing binary at `${BIN_DIR}/openpanel` is backed up to
   `${BIN_DIR}/openpanel.bak.<UTC-timestamp>`.

If the swap fails (e.g. `install` returns non-zero, or the
post-swap smoke check fails), the installer restores the backup
automatically. The rollback subcommand is a manual escape hatch for
the case where the smoke check accepts the new binary but the
operator decides the deploy is unsafe.

## CI gates

Every push and pull request runs `make check` (see
`openspec/specs/quality/spec.md`). The release workflow runs the
same gate before any artifact is built, so a broken tree never
produces a release artifact. The release-governance gate is the
final pre-publication check; a failed provenance, a missing sidecar,
or a signature that does not verify (when `RELEASE_VERIFY_KEY` is
set) blocks the upload step.

## Operator checklist before promoting a release

- [ ] `make check` is green on the release commit.
- [ ] `release.yml` ran on the tag and produced the full matrix.
- [ ] Every artifact in `dist/` has `.sha256` + `.sbom.json` + `.sig`
      and is listed in `provenance.json`.
- [ ] The dry-run reproducibility job (or a local reproduction) shows
      identical SHA-256 across two clean builds.
- [ ] `packages/installer/smoke-container.sh` passes against the
      container image.
- [ ] The release notes include the public-key fingerprint and the
      per-target SHA-256 lines.
