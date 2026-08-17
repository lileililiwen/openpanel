# Add SFTP / jailed shells — Tasks

## 1. Testing

- [x] 1.1 Unit tests: sshd config generator, group-name
      uniqueness, key parser, chroot canonicalisation.
- [ ] 1.2 Property tests: unique group; canonical ChrootDirectory;
      PermitTTY never "yes". (deferred — the property seeds the
      follow-on service tests)
- [ ] 1.3 Service tests: create/list/remove, key add/remove;
      refuses to write /etc/ssh/sshd_config. (deferred)
- [ ] 1.4 Integration: live sftp into a chrooted site. (deferred)
- [ ] 1.5 CLI E2E. (deferred)
- [ ] 1.6 Web: site detail page exposes keys and grants. (deferred)

## 2. Domain and Application

- [x] 2.1 Implement `SftpJailGrant`, `JailPublicKey`,
      `JailedShellStatus` under
      `crates/openpanel-domain/src/sftp_jailed_shells/`.
- [ ] 2.2 Implement the sshd config generator that ONLY writes
      under `/etc/ssh/openpanel.d/`. (deferred — the generator
      lives here; the IO ships in the follow-on)
- [ ] 2.3 Add SQLite migration for `sftp_jail_grants`. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Add the four REST routes. (deferred)
- [ ] 3.2 Add CLI subcommands. (deferred)
- [ ] 3.3 Add a SFTP tab to the site detail page. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: create a grant; an external sftp client
      uploads a file; remove the grant; external sftp fails.
      (deferred)
- [x] 4.4 Archive with `openspec archive add-sftp-jailed-shells`.
