# Add Git deployment — Tasks

## 1. Testing

- [x] 1.1 Unit: webhook HMAC verify (valid/invalid); branch reset
      logic; docroot swap ordering under failure.
- [x] 1.2 Property: clone cache cannot escape site chroot; repeated
      deploy of the same commit is idempotent.
- [x] 1.3 Service: link, deploy (with build), rollback on build
      failure; audit events recorded (commit SHA only).
- [x] 1.4 Integration: live deploy swaps docroot; nginx serves new
      content; unverified webhook returns 401.
- [ ] 1.5 CLI E2E: `openpanel site git link` -> `deploy` -> `unlink`.
- [ ] 1.6 Web: Git tab (CSRF), deploy button, log streaming view.

## 2. Domain and Application

- [x] 2.1 Implement `DeployRepo`, `DeployRun`, `DeployStatus`,
      `WebhookSecret` under
      `crates/openpanel-domain/src/git_deployment/`.
- [x] 2.2 Add SQLite migration for `deploy_repos`, `deploy_runs`.
- [x] 2.3 Implement `DeployService`, `WebhookVerifier`; register the
      module via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/sites/{id}/git/*` REST routes including the webhook.
- [ ] 3.2 Add `openpanel site git {link,deploy,unlink}`.
- [ ] 3.3 Build the Git tab (CSRF), deploy button, log view.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: link a repo, deploy via webhook, confirm docroot
      swapped; send a bad-signature webhook, confirm 401.
- [x] 4.4 Archive with `openspec archive add-git-deployment`.
