# Add Preview deployments

## Why

Coolify, Dokploy, Easypanel and CapRover all build ephemeral
pull-request environments; no classic hosting panel does. OpenPanel
already has the two halves — signed branch webhooks (`git-deployment`)
and staging slots with atomic promote (`site-staging`) — but nothing
connects a PR to a disposable environment. This is OpenPanel's clearest
differentiator opportunity against both cohorts: PaaS-grade previews
with classic-panel site management.

## What Changes

- **Preview lifecycle**: a pull-request webhook (opened/synchronize)
  creates or updates an ephemeral preview slot bound to
  `(repo, branch, pr)`; closing/merging tears it down.
- **Preview URL**: deterministic `<pr>.<preview-base-domain>` host
  with TLS via the existing ACME stack (wildcard DNS-01 preferred).
- **Isolation**: previews run against isolated runtime slots and
  database placeholders; they cannot reach production resources.
- **Limits**: per-repo concurrent preview cap and TTL expiry enforced
  by a background reaper; plan quotas apply.
- **Status reporting**: optional signed status callback to the forge.
- Surfaces: API routes under `/api/v1/sites/{id}/previews`, CLI
  `openpanel site preview …`, web Previews tab.

## Capabilities

### Modified Capabilities

- `git-deployment`: add PR-triggered ephemeral environments on top of
  branch deploys.

## Impact

- Domain: `PreviewEnvironment{repo_ref, pr_number, slot_id, url,
  state, expires_at}`, `PreviewError`; state machine
  Creating→Ready→Failed→Expired|Destroyed.
- App: extends git-deployment service (webhook dispatch gains a PR
  event arm) and site-staging service (slots gain `preview` purpose);
  reaper as background task; DNS record creation via existing dns
  module port; certificate via ssl wildcard flow.
- API/CLI/web: new routes/subcommands/tab.
- Security: webhook signature verification reused verbatim; preview
  URLs never expose internal ids beyond the PR number; teardown is
  audited.
- Coupling: git-deployment, site-staging, dns, ssl, app-runtimes,
  quotas, cron (TTL).

## Non-goals

- No per-PR database seeding/migration execution (placeholder DBs
  only).
- No monorepo path-filtering triggers (Dokploy-style) — later.
- No review-app comment posting beyond an optional status callback.
