# Add admin IP allowlist — Tasks

## 1. Testing

- [x] 1.1 Unit tests: CIDR parse, IPv4/IPv6 matching, mode
      behaviour, bypass-token constant-time.
- [x] 1.2 Property tests: strict vs or-open modes.
- [ ] 1.3 Service tests: PUT/GET/test; bypass expired token. (deferred)
- [ ] 1.4 Integration: middleware blocks non-matching IP; consulted
      before session auth. (deferred)
- [ ] 1.5 CLI E2E: set, test, remove. (deferred)
- [ ] 1.6 Web: admin security tab. (deferred)

## 2. Domain and Application

- [x] 2.1 Implement `AdminIpAllowlist`, `IpCidr`,
      `AllowlistMode`, `AllowlistOverride` under
      `crates/openpanel-domain/src/admin_ip_allowlist/`.
- [ ] 2.2 Implement `IpAllowlistMiddleware` in
      `crates/openpanel-api/src/middleware/`. (deferred)
- [ ] 2.3 Add SQLite migration for `admin_ip_allowlist`. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Add `/api/v1/admin/security/ip-allowlist/*`. (deferred)
- [ ] 3.2 Add CLI subcommands. (deferred)
- [ ] 3.3 Add the admin security tab. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: configure strict mode with a single
      matching CIDR; non-matching IP is 403. (deferred)
- [x] 4.4 Archive with `openspec archive add-admin-ip-allowlist`.

## 5. Module Wiring

- [x] 5.1 Extend `Role` with `PartialOrd` + `Ord` so it can be a
      `BTreeMap` key (used by the role-overrides index).
