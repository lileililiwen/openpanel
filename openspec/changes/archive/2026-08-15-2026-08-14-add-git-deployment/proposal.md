# Add Git deployment

## Why

OpenPanel already provisions sites, PHP runtimes, staging, and
clone/template export, but has **no first-class Git deployment** path.
Modern hosts (Plesk Git, cPanel Git™ Version Control, CloudPanel,
1Panel, Forge, RunCloud) let a developer link a repository, pick a
branch, and deploy on push via webhook — or on demand — with atomic
swaps and clear build steps. Without it, OpenPanel users fall back to
SFTP uploads or manual `git pull` over SSH, losing auditability,
atomicity, and rollback. This change adds a `git-deployment` bounded
context.

## What Changes

- New bounded context `git-deployment` carrying the `DeployRepo`
  aggregate and `DeployService`, `WebhookVerifier`.
- New endpoints: `POST /sites/{id}/git/link`,
  `GET /sites/{id}/git`, `POST /sites/{id}/git/deploy`,
  `POST /sites/{id}/git/webhook`, `DELETE /sites/{id}/git`.
- Link a repo (HTTPS with a stored token, or SSH key) to a site
  branch; a deploy fetches the branch, optionally runs a build script,
  then performs an atomic docroot swap (reusing the `site-staging`
  rename pattern).
- A webhook receiver verifies an HMAC signature before triggering a
  deploy; every deploy is audited and reversible.

## Capabilities

### New Capabilities

- `git-deployment`: link a repository/branch to a site, deploy on
  demand or via signed webhook, with atomic swap and audit trail.

## Impact

- Domain: `DeployRepo`, `DeployRun`, `DeployStatus`, `WebhookSecret`.
- App: `DeployService`, `WebhookVerifier`, `DeployFilesystemLayer`.
- API/CLI/web: `/sites/{id}/git/*`; CLI
  `openpanel site git {link,deploy,unlink}`; web Git tab (CSRF).
- Security: repo tokens / SSH private keys encrypted at rest under the
  master key; webhook secrets HMAC-SHA256; the webhook endpoint is
  signed-only and never executes unverified input.
- Coupling: depends on `sites` for chroot / docroot; reuses the atomic
  rename pattern from `site-staging`; `JobSupervisor` runs build
  scripts.
