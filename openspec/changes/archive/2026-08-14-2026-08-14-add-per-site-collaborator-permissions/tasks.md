# Add Per-site collaborator permissions — Tasks

## 1. Testing

- [x] 1.1 Unit: permission-set union across multiple grants; scope bit
      masking for file/db/mail/cron.
- [x] 1.2 Property: a collaborator's effective access never equals the
      account role; revoke removes all access for the site.
- [x] 1.3 Service: invite a collaborator; resolve access for a site;
      revoke and confirm loss of access.
- [x] 1.4 Integration: a collaborator can act only within granted scope;
      revoke blocks previously granted actions.
- [x] 1.5 Web: Collaborators tab (CSRF), invite form, scope toggle
      controls.

## 2. Domain and Application

- [x] 2.1 Implement `Collaborator`, `SiteGrant`, `PermissionSet` under
      `crates/openpanel-domain/src/collaborators/`.
- [x] 2.2 Add SQLite migration for `collaborators`, `site_grants`.
- [x] 2.3 Implement `CollaboratorService`, `GrantResolver`; register via
      `ModuleRegistry`; integrate with `identity` and `account-hierarchy`.

## 3. Adapters and UI

- [x] 3.1 Add `/sites/{id}/collaborators` and
      `/sites/{id}/collaborators/{uid}` REST routes (POST/GET/PUT/DELETE).
- [x] 3.2 Build the Collaborators tab (CSRF), invite form, scope
      toggles.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (clippy pre-existing).
- [x] 4.3 Smoke-test: invite a collaborator to one site with file +
      db scope; confirm mail/cron are denied; revoke and confirm full
      loss of access.
- [x] 4.4 Archive with `openspec archive add-per-site-collaborator-permissions`.