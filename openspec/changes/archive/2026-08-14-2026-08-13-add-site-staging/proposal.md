# Add site staging

## Why

`refine-sites-with-multi-php-clone-fields` introduced the
`instance_origin_id` so a cloned site can be tracked, and
`add-site-clone-and-template-export` will use that field. Sites that
ship code, however, also need a safe place to test changes
before they hit production. Modern hosts (Forge, RunCloud,
Laravel Vapor) ship **per-site staging** at a subdomain like
`staging.<domain>`, with promotion (atomic swap) when ready.
OpenPanel currently lacks that abstraction; this change adds it.

## What Changes

- New bounded context `site-staging` carrying the
  `StagingSlot` aggregate and `StagingPromoter` service.
- New endpoints: `POST /sites/{id}/staging/create`,
  `POST /sites/{id}/staging/sync`,
  `POST /sites/{id}/staging/promote`.
- New role semantics: staging shares the production site's
  PHP runtime but exposes traffic at
  `staging.<primary_domain>` (configurable).
- Atomic promote: swap document roots under a chroot-validated
  rename; one transaction, one nginx reload.

## Capabilities

### New Capabilities

- `site-staging`: per-site staging slot with sync and atomic
  promote.

## Impact

- Domain: `StagingSlot`, `PromotionRun`, `PromotionStatus`.
- App: `StagingService`, `PromotionService`,
  `StagingSlotFilesystemLayer` (lives under
  `/var/www/<domain>/staging/`).
- API/CLI/web: `/sites/{id}/staging/*`; CLI
  `openpanel site staging {create,sync,promote}`; web staging
  tab (CSRF).
- Coupling: depends on the `sites` cap for chroot and on
  `per-site-php-runtime` for runtime sharing.
