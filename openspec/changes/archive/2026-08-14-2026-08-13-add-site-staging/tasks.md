# Add site staging — Tasks

## 1. Testing

- [x] 1.1 Unit tests: chroot validation; staging DB naming;
      promotion transaction ordering.
- [x] 1.2 Property tests: staging cannot escape chroot;
      idempotent sync per snapshot id.
- [x] 1.3 Service tests: snapshot, promote, rollback, lock.
- [x] 1.4 Integration: live promote across nginx; rollback on
      failure.
- [x] 1.5 CLI E2E: create → sync → promote → destroy.
- [x] 1.6 Web: staging tab (CSRF), promote confirmation.

## 2. Domain and Application

- [x] 2.1 Implement `StagingSlot`, `PromotionRun`,
      `PromotionStatus` under
      `crates/openpanel-domain/src/site_staging/`.
- [x] 2.2 Add SQLite migration for `staging_slots`,
      `staging_snapshots`, `promotion_runs`.
- [x] 2.3 Implement `StagingService`, `PromotionService`.

## 3. Adapters and UI

- [x] 3.1 Add `/sites/{id}/staging/*` REST routes.
- [x] 3.2 Add `openpanel site staging {create,sync,promote,delete}`.
- [x] 3.3 Build the staging tab (CSRF), promote confirmation.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: snapshot a site; promote; nginx serves
      the new content; trigger a failure (nginx -t blocked)
      and observe rollback.
- [x] 4.4 Archive with `openspec archive add-site-staging`.
