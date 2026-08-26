# Refine Logs with rotation policy — Design

## Explore & Reuse

- `openspec/specs/logs/spec.md` — Authorized Log Sources defines the
  source classes this policy addresses; Bounded Log Reading governs
  reading rotated archives too.
- Rotation detection already exists in the logs app layer — extended
  to compare observed state against the rendered policy (drift flag).
- Atomic write precedent: sites vhost writer; managed-block markers
  precedent: host-security authorized_keys renderer.
- Binary detection convention: `which logrotate` at startup mirroring
  the mysql detection pattern (`DatabaseError::MysqlMissing` analogue
  → `LogsError::LogrotateMissing`, mutations refused).
- Audit via AuditService.

## Model

```rust
pub struct RotationPolicy {
    source_class: SourceClass,   // SiteAccess | SiteError | ManagedService | Panel
    max_age_days: u16,           // 1..=365
    max_size_mb: u32,            // 1..=4096
    keep_generations: u8,        // 1..=52
    compress: bool,
}
```

Bounds validated in domain; one policy per source class (upsert).

## Rendering

```
/etc/logrotate.d/openpanel-<class>:
  <paths for class> {
    daily
    maxage <d>
    maxsize <m>M
    rotate <n>
    <compress|nocompress>
    missingok
    notifempty
    sharedscripts
    postrotate  # nginx reopen / service signal where applicable
  }
```

Paths resolved from the same source registry the reader uses; postrotate
signals only for allowlisted services (system-services inventory).

## Manual rotation

`POST /api/v1/logs/policies/{class}/rotate` runs
`logrotate --force /etc/logrotate.d/openpanel-<class>` with output cap;
result audited.

## Drift flag

Existing rotation-detection compares on-disk rotated files against
policy expectations (count/age/compression) and surfaces a boolean in
the policy DTO.

## Endpoints / CLI / Web

```
GET/PUT /api/v1/logs/policies[/{class}]
POST    /api/v1/logs/policies/{class}/rotate
CLI: openpanel logs policy {show,set,rotate}
Web: Logs → Retention tab
```

## Layering

Domain: pure VO/validation. App: renderer/writer/service/repo.
Adapters standard.
