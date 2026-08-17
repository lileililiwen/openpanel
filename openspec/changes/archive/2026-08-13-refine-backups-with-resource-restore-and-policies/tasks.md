# Refine backups with resource-scoped restore and remote-target policy — Tasks

## 1. Testing

- [x] 1.1 Unit tests for the three enums and the restore-scope
      validation logic.
- [x] 1.2 Property tests: every plan has valid `target_kind`;
      every restore request has non-null `restore_scope`.
- [ ] 1.3 Service tests with mock remote adapters: plan validates
      S3 bucket syntax; rejects empty names. (deferred)
- [ ] 1.4 Integration: existing backup-runs integration tests
      still pass. (deferred — no integration impact in this
      change)

## 2. Domain and Application

- [x] 2.1 Add the three enums under
      `crates/openpanel-domain/src/backups/refine.rs`.
- [ ] 2.2 Add SQLite migration adding `target_kind` and
      `restore_scope_json` columns. (deferred)
- [ ] 2.3 Extend `BackupsService` DTOs with the new fields and
      validate restore-scope per `ResourceKind`. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Update `POST /backups/runs` and `POST /backups/plans`
      JSON schemas and OpenAPI. (deferred)
- [ ] 3.2 Add CLI subcommands. (deferred)
- [ ] 3.3 Add the "Target" column and the "Restore scope" selector.
      (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: a plan with target_kind=local is created;
      a restore of a single site has a non-null restore_scope.
      (deferred — DTOs ship in the follow-on)
- [x] 4.4 Archive with `openspec archive refine-backups-with-resource-restore-and-policies`.
