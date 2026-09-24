# Proposal: Repair release security and evidence gates

## Why

OpenPanel has release and quality governance, but the current dependency audit
blocks on an actionable `rustls` security advisory and several important
checks can remain skipped outside CI. A public-benefit project needs release
claims that are reproducible, security-reviewed, and backed by evidence rather
than by local workstation state.

## What Changes

- Upgrade and lock audited security-sensitive dependencies.
- Make release, coverage, browser, and dependency evidence explicit and
  fail-closed for required publication paths.
- Record artifact provenance, tool versions, environment, and skipped checks.
- Keep local developer convenience checks separate from release publication
  gates.

## BFS Impact Map

- Capabilities: `quality-maturity-ratchet`, `release-deployment-governance`,
  `browser-ui-quality`, and `quality` are modified.
- Users: maintainers, release operators, downstream self-hosters, and
  security reviewers.
- Contracts: Cargo lockfile, CI workflows, release metadata, coverage and
  browser artifacts, and gate exit codes.
- Integrations: RustSec, CI runners, coverage/browser tooling, and artifact
  signing; no desktop or Jenkins dependency.
- Failure boundaries: advisory found, tool missing, stale artifact, invalid
  signature, and incomplete provenance MUST be distinguishable.
- Unaffected: application routes, domain behavior, data migrations, and
  customer deployment topology.

## Capabilities

### Modified Capabilities

- `quality-maturity-ratchet`
- `release-deployment-governance`
- `browser-ui-quality`
- `quality`

## Non-goals

- No product feature implementation.
- No assumption that a maintainer owns a server, Mac, Docker Desktop, or
  Jenkins.
- No automatic acceptance of a known advisory because it is difficult to
  reproduce locally.
