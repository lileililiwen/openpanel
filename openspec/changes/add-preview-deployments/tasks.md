# Add Preview deployments — Tasks

## 1. Testing

- [ ] 1.1 Unit: state machine — `Creating→Building→Ready` legal;
      `Ready→Destroyed` on close/expiry; any transition out of
      `Destroyed` is illegal and returns `PreviewError::AlreadyGone`.
- [ ] 1.2 Unit: preview hostname derivation — PR 42 on base domain
      `pr.example.com` yields exactly `42.pr.example.com`; PR numbers
      ≤0 or non-numeric rejected.
- [ ] 1.3 Unit: TTL computation — `expires_at = ready_at + ttl_hours`;
      reaper selects only expired, non-destroyed rows.
- [ ] 1.4 Property: for arbitrary valid `(repo, pr)` pairs the derived
      URL never collides with another live preview of the same repo
      and contains no characters outside `[a-z0-9.-]`.
- [ ] 1.5 Integration (`tests/integration/previews.rs`): signed PR
      opened webhook → preview row in Creating then Ready (mock
      builder), DNS record requested once, audit events emitted;
      replaying the same webhook updates rather than duplicates.
- [ ] 1.6 Integration: PR closed webhook → slot destroyed, nginx
      fragment removed, cert untouched; `DELETE .../previews/{pr}` by a
      collaborator without scope → 403.
- [ ] 1.7 Integration: creating max_per_repo+1 previews evicts the
      oldest expired one or rejects with `PreviewError::CapReached`.
- [ ] 1.8 Integration: preview request to production database name
      fails isolation check (placeholder only) — assert connection
      target differs from production.
- [ ] 1.9 CLI E2E: `cli_site_preview_list_shows_pr_and_state`.
- [ ] 1.10 Web: Previews tab at 360/768/1280 px; screenshots in PR.

## 2. Domain

- [ ] 2.1 Add `PreviewEnvironment`, state machine, `PreviewError`,
      repo-trait extensions under
      `crates/openpanel-domain/src/git_deployment/`.

## 3. Application

- [ ] 3.1 Extend webhook dispatcher with `pull_request` arm (signature
      verification unchanged).
- [ ] 3.2 Preview service: upsert/build/destroy against staging-slot
      ports; wildcard DNS record via dns module port; wildcard cert
      reuse via ssl module.
- [ ] 3.3 TTL reaper background task; per-repo cap enforcement.
- [ ] 3.4 Optional signed status callback sender.

## 4. Adapters and UI

- [ ] 4.1 REST routes `/api/v1/sites/{id}/previews*`.
- [ ] 4.2 CLI `openpanel site preview …`.
- [ ] 4.3 Web Previews tab.
- [ ] 4.4 nginx renderer emits preview server blocks (sites pipeline).

## 5. Validation

- [ ] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean.
- [ ] 5.3 Smoke-test against a scratch GitHub repo: open PR → preview
      URL serves over HTTPS; close PR → 404 and resources gone.
- [ ] 5.4 Archive with `openspec archive add-preview-deployments`.
