## ADDED Requirements

### Requirement: Preview Environment Lifecycle

The system SHALL create or update an ephemeral preview environment
when a signed pull-request webhook (`opened`, `synchronize`) arrives
for a linked repository, and SHALL destroy it when the PR is closed or
merged. Replayed webhooks for an existing `(repo, branch, pr)` SHALL
update in place rather than duplicate.

#### Scenario: PR opened creates preview

- **WHEN** a validly signed `pull_request.opened` webhook arrives
- **THEN** exactly one preview environment is created in Creating
          state and an audit event records it.

#### Scenario: Replay updates

- **WHEN** the same PR webhook is delivered twice
- **THEN** only one preview environment exists for that PR afterwards.

#### Scenario: PR closed destroys

- **WHEN** a `pull_request.closed` webhook arrives
- **THEN** the preview slot, routing fragment, and runtime unit are
          destroyed and audited with reason `pr_closed`.

### Requirement: Deterministic Preview URL

Each preview SHALL be reachable at `<pr>.<preview-base-domain>` over
TLS covered by a wildcard certificate for that base domain; per-PR
HTTP-01 issuance SHALL NOT be attempted.

#### Scenario: URL derivation

- **WHEN** PR 42 is built on base domain `pr.example.com`
- **THEN** the preview serves at `https://42.pr.example.com` using the
          existing wildcard certificate.

### Requirement: Preview Isolation

Preview environments SHALL use isolated slots and database
placeholders namespaced by PR, SHALL be excluded from production
promote paths, and SHALL NOT count against production resource usage
counters.

#### Scenario: No production access

- **WHEN** code inside a preview attempts to open the production
        database name
- **THEN** the connection resolves to the PR placeholder only, never
        the production database.

### Requirement: Preview Limits and Expiry

The system SHALL enforce a per-repository concurrent-preview cap and a
TTL after which a background reaper destroys expired previews;
plan quotas SHALL apply to preview counts.

#### Scenario: Cap enforced

- **WHEN** more than `max_per_repo` live previews are requested for
        one repository
- **THEN** creation fails with `PreviewError::CapReached` unless an
          expired preview can be reclaimed first.

#### Scenario: TTL expiry

- **WHEN** a preview has been Ready longer than `ttl_hours`
- **THEN** the reaper destroys it and audits
          `PreviewDestroyed{reason: "expired"}`.
