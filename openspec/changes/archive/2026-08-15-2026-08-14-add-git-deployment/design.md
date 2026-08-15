# Add Git deployment — Design

## DeployRepo model

```rust
pub struct DeployRepo {
    pub site_id: SiteId,
    pub repo_url: String,              // https or git@host:org/repo
    pub branch: String,                // default "main"
    pub auth: RepoAuth,                // HttpsToken | SshKey (ciphertext)
    pub docroot_target: PathBuf,       // deploy destination under site chroot
    pub build_script: Option<String>,  // run after checkout
    pub webhook_secret: WebhookSecret, // HMAC-SHA256 key
    pub last_run_id: Option<DeployRunId>,
}
```

## Filesystem / clone layout

```
/var/www/<domain>/.git-deploy/        clone cache (0700, owner-only)
/var/www/<domain>/.git-deploy/.next/  staged build output
```

The swap reuses the `site-staging` rename chain: build into `.next`,
then atomically rename into `docroot_target`, then `nginx -t &&
nginx -s reload`.

## Deploy flow

```
deploy(site_id, triggered_by):
  git fetch + reset --hard <branch> (clone on first run)
  if build_script: run under JobSupervisor, 5-min cap, captured logs
  atomic swap: rename built tree -> docroot_target
  audit DeployRun{status, commit, triggered_by}
  on failure: reverse rename chain, audit DeployRolledBack
```

## Webhook

```
POST /sites/{id}/git/webhook
  verify HMAC-SHA256(body, webhook_secret) == X-Signature
  on mismatch -> 401, no deploy
  on match    -> enqueue deploy (async)
```

## Endpoints

```
POST /api/v1/sites/{id}/git/link    body { repo_url, branch?, auth, build_script? }
GET  /api/v1/sites/{id}/git
POST /api/v1/sites/{id}/git/deploy  body { ref? }
POST /api/v1/sites/{id}/git/webhook header X-Signature
DELETE /api/v1/sites/{id}/git
```

## Tests

```
1.1 Unit: webhook HMAC verify; branch reset; docroot swap ordering.
1.2 Property: clone cache stays inside site chroot; deploy idempotent.
1.3 Service tests w/ mock git + mock fs: link, deploy, rollback.
1.4 Integration: live deploy swaps docroot; bad signature refused.
1.5 CLI E2E: link -> deploy -> unlink.
1.6 Web: Git tab (CSRF), deploy button + log view.
```
