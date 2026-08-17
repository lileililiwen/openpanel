# git-deployment Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-git-deployment. Update Purpose after archive.
## Requirements
### Requirement: Link Repository

The system SHALL let an authorised caller link a Git repository
(HTTPS with a stored token, or an SSH key) and a branch to a site.
Credentials SHALL be encrypted at rest under the master key; the
webhook secret SHALL be HMAC-SHA256. The deploy target MUST resolve
inside the site chroot.

#### Scenario: Link succeeds

- **WHEN** an Owner posts `POST /sites/{s1}/git/link` with a valid
        `repo_url` and `branch`
- **THEN** a `DeployRepo` row exists, credentials are ciphertext, and
        an audit `GitRepoLinked` is recorded.

#### Scenario: Link outside chroot rejected

- **WHEN** `docroot_target` resolves outside the site chroot
- **THEN** the request is rejected with `GitDeployError::OutsideChroot`.

### Requirement: On-Demand Deploy

`POST /sites/{id}/git/deploy` SHALL fetch the branch, optionally run a
build script, and atomically swap the built tree into the docroot,
reusing the `site-staging` rename chain. The run SHALL be audited with
the resolved commit SHA (hash only, never the diff).

#### Scenario: Successful deploy

- **WHEN** an Owner deploys site `s1`
- **THEN** the docroot serves the fetched commit; audit
        `GitDeployRun{commit, status=success}` records the SHA only.

#### Scenario: Build failure rolls back

- **WHEN** the build script exits non-zero
- **THEN** no docroot swap occurs; audit `GitDeployRolledBack{run_id}`.

### Requirement: Signed Webhook

`POST /sites/{id}/git/webhook` SHALL verify
`HMAC-SHA256(body, webhook_secret)` against the `X-Signature` header
before enqueuing a deploy. An unverified request SHALL return `401`
and SHALL NOT deploy.

#### Scenario: Valid signature deploys

- **WHEN** a webhook arrives with a correct signature
- **THEN** a deploy is enqueued and `202 Accepted` is returned.

#### Scenario: Bad signature refused

- **WHEN** the signature is invalid or absent
- **THEN** the response is `401` and no deploy runs; audit
        `GitWebhookRejected{reason}` records the reason only.

### Requirement: Unlink

`DELETE /sites/{id}/git` SHALL remove the `DeployRepo` row, delete the
clone cache, and revoke the webhook secret.

#### Scenario: Unlink

- **WHEN** an Owner deletes the link
- **THEN** the row, clone cache, and secret are gone; subsequent
        webhooks return `404`.

