# openpanel Roadmap

Record dependency-ordered delivery phases, release targets, deferred scope, and evidence boundaries here.

## Portable real-world production roadmap

This roadmap is product-portable. A maintainer workstation, macOS, Docker
Desktop, Jenkins, Cloudflare, or `/Users/allen` may be used as an optional
adapter/conformance environment, but no product requirement may depend on it.

1. `repair-release-security-and-evidence` — repair actionable dependency
   advisories and make required release evidence fail closed.
2. `add-portable-runtime-packaging` — define equivalent native Linux and OCI
   runtime contracts, persistent data, health, upgrade, and rollback.
3. `add-portable-deployment-adapters` — define provider-neutral deployment
   plans, idempotency, evidence, and adapter capability declarations.
4. `add-git-application-delivery` — unify Git/OCI/Compose application
   delivery, immutable releases, health-gated promotion, previews, and
   rollback.
5. `add-verified-service-catalog` — provide signed, reviewable ecosystem
   templates with safe install, upgrade, backup, and removal.
6. `add-portable-host-operations-and-migration` — add portable host
   capability, migration previews, panel import drivers, and verified scoped
   cutover.

### Deferred and explicitly out of scope

- Mac/Jenkins-specific behavior is an adapter test target, not product scope.
- Public DNS, Cloudflare, mail-provider credentials, and host firewalls stay
  operator-owned integrations.
- Kubernetes, object storage, CalDAV/CardDAV/WebDAV, and mailing-list
  moderation require separate proposals if later justified.

### Evidence boundary

Strict OpenSpec validation proves planning structure only. Runtime readiness
requires a clean supported Linux or OCI target, signed artifacts, health and
rollback evidence, browser evidence, backup/restore rehearsal, and a recorded
deployment result.
