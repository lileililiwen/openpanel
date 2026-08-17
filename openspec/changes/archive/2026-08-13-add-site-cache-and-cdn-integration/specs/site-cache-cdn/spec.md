## Purpose

Adds per-site page cache (nginx `proxy_cache`) and CDN
integration (Cloudflare / CloudFront / generic HTTP purge) so
operators can ship sites that hit behind an edge. The cache
policy is typed and rejects unsafe bypass globs; the adapter
trait keeps the panel decoupled from a specific vendor.

# site-cache-cdn Specification

## Requirements

### Requirement: Site Cache Policy

The system SHALL let authorised callers set a typed
`SiteCachePolicy` per site. The policy MUST include
`ttl_seconds`, `static_assets_ttl_seconds`,
`bypass_paths`, `keyed_cookies`, and `stale_while_revalidate`.
On apply, the panel SHALL render an nginx vhost snippet that
honours the policy and SHALL reload nginx only after
`nginx -t` returns 0.

#### Scenario: Cache policy applied

- **WHEN** an Owner PUTs a policy with `ttl_seconds=60`
- **THEN** the nginx snippet is written under the site's
        panel-owned include path; `nginx -t` passes; nginx
        reloads; audit `SiteCachePolicyApplied` records the
        site id and TTL only.

#### Scenario: Bypass glob applied

- **WHEN** `bypass_paths = ["/wp-admin", "/api/*"]`
- **THEN** requests matching the globs return `BYPASS` from
        nginx and are never written to the cache.

#### Scenario: Syntax error

- **WHEN** `nginx -t` fails after a write
- **THEN** the prior snippet is restored atomically; nginx
        reload is rolled back; audit `SiteCachePolicyRolledBack`
        is recorded.

### Requirement: Cache Purge

The system SHALL support purging a list of paths under a site
in a single call, regardless of whether the cache key
includes query strings or cookies. Purge is idempotent;
calling it twice on the same path is a no-op.

#### Scenario: Purge a path

- **WHEN** an Owner runs `cache.purge("/article/x")`
- **THEN** the cache entry is removed; subsequent requests
        are re-fetched from the origin.

#### Scenario: Idempotent

- **WHEN** the same path is purged twice
- **THEN** the second call returns success with no entries
        removed; no audit event is recorded the second time.

### Requirement: CDN Adapter Registry

The system SHALL expose a `CdnAdapterRegistry` with concrete
adapters for Cloudflare, CloudFront, and a generic HTTP
webhook. Each integration record MAY have a credential
stored encrypted (per the offsite-backup-targets model) and
a `webhook_url` payload.

#### Scenario: Register Cloudflare

- **WHEN** an Owner posts `{ kind: "cloudflare", api_token, zone_id }`
- **THEN** the integration is persisted with the credential
        encrypted; `list_zones` is callable.

#### Scenario: Adapter not loaded

- **WHEN** the integration specifies a kind for which the binary has no adapter compiled in
- **THEN** the integration is rejected with `CdnError::AdapterNotAvailable`; no subprocess is launched.

### Requirement: CDN Purge Routing

`POST /api/v1/cdn/purge` SHALL accept an integration id and a
list of URLs/paths. The adapter MUST translate paths into
zone-shaped requests (CloudFront invalidations, Cloudflare
cache-tags) and SHALL return a typed `PurgeReceipt`. A
partial failure SHALL emit a `CdnPurgePartial{successful,
failed, redacted}` and continue with the rest.

#### Scenario: Successful purge

- **WHEN** an Owner submits `{ integration_id, paths: ["/article/x", "/y"] }` to a healthy Cloudflare integration
- **THEN** the response is `{ successful: 2, failed: 0, redacted: [] }`.

#### Scenario: Partial failure

- **WHEN** one of two paths returns a 400 from the upstream
- **THEN** the response is `{ successful: 1, failed: 1, redacted: ["path=/y reason=bad_request"] }`; audit `CdnPurgePartial` records path names only.

### Requirement: Audit and Secret Hygiene

Every cache mutation and CDN call SHALL be audited with
`{ actor_id, target, kind, redacted_request }`. Plaintext CDN
API tokens SHALL NEVER appear in any log, error, audit, API,
or web response.

#### Scenario: Token never echoed

- **WHEN** a Cloudflare integration is read back via the API
- **THEN** the response is `{ id, kind, zone_id }` and the
        API token is omitted.

#### Scenario: Tampered credential

- **WHEN** the encrypted CDN credential blob is tampered at rest
- **THEN** the next call returns `CdnError::CryptoFailure` and
        audit `CdnCredentialDecryptFailed` records only the
        integration id.
