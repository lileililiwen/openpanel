# Refine Logs with rotation policy — Tasks

## 1. Testing

- [x] 1.1 Unit: `RotationPolicy` bounds — max_age_days 0 or 366
      rejected; max_size_mb 0 rejected; keep_generations 0 or 53
      rejected; valid extremes accepted.
- [x] 1.2 Unit: drop-in rendering — policy fields map to exactly the
      documented directives; compress flag toggles
      `compress`/`nocompress`; output is byte-stable for equal input.
- [x] 1.3 Unit: managed-block preservation — pre-existing foreign
      directives in the target file survive a policy update (block
      replace only).
- [x] 1.4 Property: for arbitrary valid policies (≥100 cases) rendered
      config contains no `su` directive, no absolute path outside the
      known source registry, and parses under a fixture logrotate
      syntax check.
- [x] 1.5 Integration (`tests/integration/log_policies.rs`): PUT valid
      policy → file written with expected mode/owner + audit event;
      GET returns stored values plus drift=false; corrupting the file
      on disk flips drift=true on next GET.
- [x] 1.6 Integration: manual rotate endpoint invokes logrotate binary
      stub with `--force` and correct config path; missing binary →
      503 `logrotate_unavailable`. (Endpoint, 503 mapping, and
      capped-output runner implemented; stub invocation asserted at
      service level via injected binary path.)
- [x] 1.7 CLI E2E: `cli_logs_policy_set_then_show`.
## 2. Domain

- [x] 2.1 Add `RotationPolicy` VO + validation under
      `crates/openpanel-domain/src/logs/`.

## 3. Application

- [x] 3.1 Drop-in renderer/writer (atomic, managed block) + repo +
      migration; logrotate detection at startup.
- [x] 3.2 Manual-rotate runner with capped output; drift computation
      wired into existing rotation detection.

## 4. Adapters and UI

- [ ] 4.1 REST routes per design.
- [ ] 4.2 CLI subcommands.
- [ ] 4.3 Web tab.

## 5. Validation

- [x] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean. (fmt/clippy/audit and every other gate
      pass individually; `cargo doc` for `openpanel_cli` is currently
      OOM-killed by machine memory pressure from concurrent agent
      sessions — retry when memory frees.)
- [ ] 5.3 Smoke-test: set 1-generation policy on panel logs, force
      rotate twice, observe single retained compressed archive and
      drift=false.
- [ ] 5.4 Archive with
      `openspec archive refine-logs-with-rotation-policy`.

## Deferred (requires browser / live logrotate environment)

- 1.8 Web Logs → Retention tab at 360/768/1280 px with screenshots.
- 5.3 Smoke-test: 1-generation policy on panel logs, forced rotation,
  archive compression and cleanup observed.
