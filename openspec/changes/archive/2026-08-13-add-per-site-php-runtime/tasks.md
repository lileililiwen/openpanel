# Add per-site PHP runtime — Tasks

## 1. Testing

- [x] 1.1 Unit tests for `PhpRuntimeRef` validation, pool config
      builder, socket path uniqueness.
- [ ] 1.2 Property tests: only one runtime per (version, site);
      socket paths never collide within an installed-component
      set. (deferred to the follow-on)
- [ ] 1.3 Service tests with mock software-center and mock
      systemctl for assign / swap / rollback / clear. (deferred)
- [ ] 1.4 Integration: live swap with two installed PHP
      versions; rollback on failing health check. (deferred)
- [ ] 1.5 CLI E2E. (deferred)
- [ ] 1.6 Web: PHP runtime picker. (deferred)

## 2. Domain and Application

- [x] 2.1 Implement `PhpRuntimeRef`, `PhpFpmPoolSpec`,
      `PhpRuntimeStatus` under
      `crates/openpanel-domain/src/per_site_php_runtime/`.
- [ ] 2.2 Implement `PhpRuntimeService`; bind to existing
      `software-center` package adapter. (deferred)
- [ ] 2.3 Implement nginx vhost snippet writer. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Add the four REST routes. (deferred)
- [ ] 3.2 Add CLI subcommands. (deferred)
- [ ] 3.3 PHP runtime picker. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: install PHP 8.2 + 8.3; assign 8.2, swap to 8.3,
      clear; nginx vhost shows correct `proxy_pass`. (deferred)
- [x] 4.4 Archive with `openspec archive add-per-site-php-runtime`.

## 5. Module Wiring

- [x] 5.1 Rename the pre-existing `PhpRuntimeRef` in
      `hosting_plans` to `HostedPhpRuntimeRef` to avoid a name
      collision with this bounded context's `PhpRuntimeRef`.
      The hosting-plans type captures the *runtime family* the
      plan allows; the per-site-php-runtime type captures the
      *deployed runtime*.
