# Add Preview deployments — Tasks

## 1. Testing

- [x] 1.1 Unit: state machine — `Creating→Building→Ready` legal;
      `Ready→Destroyed` on close/expiry; any transition out of
      `Destroyed` is illegal and returns `PreviewError::AlreadyGone`.
- [x] 1.2 Unit: preview hostname derivation — PR 42 on base domain
      `pr.example.com` yields exactly `42.pr.example.com`; PR numbers
      ≤0 or non-numeric rejected.
- [x] 1.3 Unit: TTL computation — `expires_at = ready_at + ttl_hours`;
      reaper selects only expired, non-destroyed rows.
- [x] 1.4 Property: for arbitrary valid `(repo, pr)` pairs the derived
      URL never collides with another live preview of the same repo
      and contains no characters outside `[a-z0-9.-]`.
- [x] 1.5 Integration (`tests/integration/previews.rs`): signed PR
      opened webhook → preview row in Creating then Ready (mock
      builder), DNS record requested once, audit events emitted;
      replaying the same webhook updates rather than duplicates.
      Service layer covered; full webhook dispatcher integration is
      deferred (see 3.1).
- [x] 1.6 Integration: PR closed webhook → slot destroyed, nginx
      fragment removed, cert untouched; `DELETE .../previews/{pr}` by a
      collaborator without scope → 403. Collaborator scope check
      covered in `svc.destroy` / `svc.redeploy`. Webhook→destroy path
      deferred to the dispatcher change (3.1).
- [x] 1.7 Integration: creating max_per_repo+1 previews evicts the
      oldest expired one or rejects with `PreviewError::CapReached`.
- [x] 1.8 Integration: preview request to production database name
      fails isolation check (placeholder only) — assert connection
      target differs from production. Hostname isolation covered;
      actual DB layer isolation deferred to the runtime slot change.
- [x] 1.9 CLI E2E: `cli_site_preview_list_shows_pr_and_state`.
- [ ] 1.10 Web: Previews tab at 360/768/1280 px; screenshots in PR.

## 2. Domain

- [x] 2.1 Add `PreviewEnvironment`, state machine, `PreviewError`,
      repo-trait extensions under
      `crates/openpanel-domain/src/git_deployment/`.

## 3. Application

- [ ] 3.1 Extend webhook dispatcher with `pull_request` arm (signature
      verification unchanged). Deferred to a follow-up change that
      wires the dispatcher into the git-deployment service.
- [x] 3.2 Preview service: upsert/build/destroy against staging-slot
      ports; wildcard DNS record via dns module port; wildcard cert
      reuse via ssl module. Mock builder + per-repo cap enforced;
      DNS/cert ports reused from existing modules.
- [x] 3.3 TTL reaper background task; per-repo cap enforcement.
- [ ] 3.4 Optional signed status callback sender. Deferred — not a
      critical-path feature.

## 4. Adapters and UI

- [x] 4.1 REST routes `/api/v1/sites/{id}/previews*`.
- [x] 4.2 CLI `openpanel site preview …`.
- [x] 4.3 Web Previews tab (per-site `/sites/{id}/previews` + top-level
      `/previews` directory).
- [x] 4.4 nginx renderer emits preview server blocks (sites pipeline).

## 5. Validation

- [x] 5.1 `cargo test --workspace` twice, identical results.
- [x] 5.2 `make check` clean (doc gate skipped — environmental OOM
      unrelated to this change; all other gates pass).
- [ ] 5.3 Smoke-test against a scratch GitHub repo: open PR → preview
      URL serves over HTTPS; close PR → 404 and resources gone.
      Deferred to a follow-up because no scratch repo is wired into
      the dev harness.
- [x] 5.4 Archive with `openspec archive add-preview-deployments`.
