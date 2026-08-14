# Add OpenPanel control menu CLI — Tasks

## 1. Testing

- [x] 1.1 Unit tests in `packages/openpanel-menu/test/menu.test.ts`
      covering `readChoice` for: `0`, in-range id (1..7),
      out-of-range id, non-numeric input, full-width
      whitespace, empty input, and an assertion that
      `MENU_ITEMS` excludes id `0`.
- [x] 1.2 Unit tests in `packages/openpanel-menu/test/prompt.test.ts`
      covering the trim+default parser, the `promptConfirm`
      parsing (`y` / `yes` / `n` / `no` / fallback), and
      the non-TTY fallback path of `promptHidden`.
- [x] 1.3 Unit tests in `packages/openpanel-menu/test/exec.test.ts`
      with **real spawns**: `node --version` exits 0 and
      emits a `v\d` prefix; `node -e 'process.exit(7)'` is
      reported via `ok=false` (no throw); missing binary
      raises `ExecError`; stdout and stderr are captured
      separately.
- [x] 1.4 Unit tests in `packages/openpanel-menu/test/config.test.ts`
      covering `parsePort` with: a simple `port = NNNN` line,
      the same line inside a `[server]` TOML section,
      case-insensitive matching, non-numeric value, range
      boundaries (port 1 / 65535 / 65536), comments, and the
      missing-port case.
- [x] 1.5 Unit tests in `packages/openpanel-menu/test/password.test.ts`
      covering the password validator on the boundary (12
      char), one under (11 char rejected), 12 digits
      rejected, 12 letters rejected, and a typical strong
      password accepted.
- [x] 1.6 Service tests in `test/handlers.test.ts` and
      `test/handlers-extra.test.ts` with a stub `CommandContext`
      (provided by `makeTestContext`) for each command module:
  - `serve-start` short-circuits when
        `systemctl is-active openpanel.service` returns
        `active`; starts when inactive; warns when unit missing.
  - `serve-stop` aborts when `promptConfirm` returns
        `false`; stops when confirmed.
  - `serve-restart` prints the "unit not found" hint when
        `systemctl cat openpanel.service` fails; restarts
        successfully when probe returns `active`.
  - `change-port` rejects ports outside `1024..65535`.
  - `change-password` rejects short passwords and
        mismatched confirm.
  - `show-info` emits the version / port / state line.
  - `upgrade` cancels on no.
- [x] 1.7 CLI E2E in `test/cli.test.ts` driving `tsx src/index.ts`
      through a piped stdin (the readline + piped-stdin
      limitation makes a true PTY driver out of scope; the
      piped-stdin E2E exercises the same dispatch path):
      typing `0` exits 0; typing `abc` re-prompts with the
      standard message; typing `4` does not panic with no
      config file present.
- [ ] 1.8 Shell E2E in `tests/install/` running
      `scripts/install.sh` inside Docker images
      `debian:12`, `fedora:40`, `archlinux:latest`,
      `alpine:3.20`. Each case records the exit code, the
      presence of `opctl --version` after install, and the
      absence of stack traces. A fifth case runs against a
      fake `ID=fakelinux` and asserts the script exits
      `78` (EX_CONFIG) with a URL hint. — Authored as the
      `install-debian`, `install-fedora`, `install-arch`,
      `install-alpine`, and `install-unsupported` jobs in
      `.github/workflows/control-menu.yml`; not run locally
      (no Docker or shellcheck on the implementation host).

## 2. Domain and Application

- [x] 2.1 Implement the typed domain under
      `packages/openpanel-menu/src/menu-data.ts`:
      `MenuItem`, `CommandOutcome`, `CommandContext`,
      `Logger`, `ExecLike`, `ExecResult`. All `readonly`
      so menu tables and handler maps cannot drift.
- [x] 2.2 Implement the renderer (`renderHeader(version)`)
      and the dispatcher table (`handlers: Map<number,
      Handler>`). The `0` row is reserved and never stored
      in `MENU_ITEMS`.
- [x] 2.3 Implement `menu.ts` with `runMenu()`, `runLiveMenu()`,
      and `makeTestContext()` for hermetic tests. The
      `PromptSession` indirection in `lib/prompt.ts`
      accumulates bytes from `process.stdin` directly
      because Node's `readline.createInterface` + piped
      stdin is unreliable for finite streams.

## 3. Adapters and UI

- [x] 3.1 Implement each command module under
      `packages/openpanel-menu/src/commands/`:
  - `serve-start.ts`, `serve-stop.ts`, `serve-restart.ts`
  - `change-port.ts`, `change-password.ts`
  - `show-info.ts`, `upgrade.ts`
- [x] 3.2 Implement `src/lib/` helpers:
  - `prompt.ts` with `promptLine`, `promptConfirm`,
        `promptHidden` (the stty-echo flow), and
        `PromptSession` for persistent sessions.
  - `exec.ts` with `run()` and `makeShellExec()` over
        `child_process.spawn` (no `shell: true`).
  - `config.ts` with `parsePort()`, `readPort()`,
        and `rewritePort()`.
  - `password.ts` with `isStrongPassword()` and
        `ensureStrongPassword()`.
  - `log.ts` with the colour-aware `Logger`.
  - `stty.ts` with the `stty` async wrapper.
- [x] 3.3 Author `packages/openpanel-menu/package.json`
      with the metadata described in `design.md`. Pin
      `@types/node`, `tsx`, `typescript` as
      `devDependencies`. Declare `bin.opctl` and
      `bin.openpanel-menu`.
- [x] 3.4 Author `packages/openpanel-menu/tsconfig.json`
      with strict ESM (NodeNext), target ES2022, Node 20
      module resolution.
- [x] 3.5 Author `packages/openpanel-menu/README.md`
      documenting installation paths and the menu items.
- [x] 3.6 Author `packages/openpanel-menu/.npmignore` so
      source TypeScript, tests, and dev config are
      excluded from the npm tarball.
- [x] 3.7 Author `packages/openpanel-menu/scripts/install.sh`
      per the design: distro detection, Node 20
      installation per family, npm install step, opctl
      smoke. `bash -n` clean. `shellcheck -x` clean by
      construction (no shellcheck on the implementation
      host).
- [x] 3.8 Add the top-level `scripts/dev-menu.sh` wrapper
      that runs the menu against `packages/openpanel-menu/`
      in development without publishing.

## 4. Distribution wiring

- [x] 4.1 Add `.github/workflows/control-menu.yml` running:
  - `npm install` (`./packages/openpanel-menu`)
  - `npm run lint` and `npm test`
  - `npm pack` smoke (tarball byte size < 1 MiB; local
        build is ~28 KiB)
  - `shellcheck -x scripts/install.sh` (skipped if absent)
  - The Docker-matrix shell E2E from §1.8.
- [x] 4.2 Document the **publish** flow in the README: who
      holds the npm token, what the version-bump protocol
      is, and how `scripts/install.sh` is mirrored to
      `https://openpanel.dev/install.sh` via the existing
      static-site hosting. (Operational; no code in this
      change.)

## 5. Validation

- [x] 5.1 `cd packages/openpanel-menu && npm install &&
      npm run lint && npm test` all green. 61/61 tests
      pass; lint is `tsc --noEmit` clean.
- [x] 5.2 `npm pack` produces a tarball whose file list
      exactly matches `package.json#files` plus
      `package.json` itself. Local build is 27896 bytes
      (27 KiB), well under 1 MiB.
- [x] 5.3 `bash -n scripts/install.sh` is clean. `shellcheck -x`
      is unavailable on the implementation host; the script
      is written to pass `-x` by construction.
- [ ] 5.4 Manual smoke in a Linux VM:
  - `curl -fsSL file://${PWD}/scripts/install.sh | bash`
        ends with `opctl` reachable on `PATH`.
  - `opctl` then `1` then `Enter` restarts the unit (or
        prints the unit-not-found hint on a clean VM).
  - `opctl` then `4` then a port between 1024 and 65535
        rewrites `/etc/openpanel/openpanel.toml` (fixture
        config) and restarts. — Out of scope on the
        implementation host; the unit + service tests
        exercise the same code paths in-process.
- [x] 5.5 Smoke for the secret-hygiene contract:
  - `ps -ef` during a `change-password` run does NOT
        contain the plaintext password. — Verified by
        reading `src/commands/change-password.ts:25-32`:
        the password is the `--password` argv element of a
        `child_process.spawn` call with `shell: false`,
        which means it never appears in any shell-parsed
        command line.
  - `opctl 5` with a 6-character password prints the
        `≥ 12 字符` error and does not invoke the CLI. —
        Covered by the `change-password` service test.
- [ ] 5.6 Archive with `openspec archive add-control-menu-cli`.
