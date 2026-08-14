# Add OpenPanel control menu CLI (Baota-style, npm + curl|bash)

## Why

OpenPanel today exposes a typed, multi-command CLI
(`openpanel-cli`) for ad-hoc operator work, but it does not
ship the **interactive numbered-menu** experience that Baota
operators have standardised on for years. Nor does it give
Linux installers a one-liner that works regardless of
distribution. Two concrete consequences:

- A first-time operator who only knows the Baota UX runs
  `opctl`, types `1`, and gets a panel restart — without
  reading any docs.
- A distro-packager or CDN sales engineer hands a customer
  one line, `curl -fsSL https://openpanel.dev/install.sh | bash`,
  and the customer is interactive-ready on any major Linux
  distribution without first installing Node.js.

This change introduces a **new sibling npm package**
`openpanel-menu` that ships exactly that UX. It is *not* a
replacement for `openpanel-cli`; it is a thin interactive
shell that, for each menu item, either delegates to
`openpanel-cli` (when a command exists) or talks to the
panel's runtime directly via systemd + config files (when a
command does not yet exist). Implementations of this
OpenSpec change produce two parts:

1. The backend package (`packages/openpanel-menu/`) — menu
   loop, typed command handlers, npm distribution metadata.
2. The distribution scripts — `scripts/install.sh` for the
   curl|bash one-liner, plus the npm `bin`, `files`, and
   `postinstall` fields on `package.json`.

OpenPanel-specific scope, decided up-front in the pre-change
requirements conversation: this menu tool wraps
`openpanel-cli` and systemd for OpenPanel. It does NOT
attempt to manage nginx, MySQL, Redis, Docker, etc.
directly — those are owned by the `software-center` and
`sites`/`databases`/`system-services` capabilities and are
configured through the panel UI.

## What Changes

- New sibling package `packages/openpanel-menu/` added to the
  openpanel monorepo layout. It is a standalone TypeScript +
  Node 20 LTS npm package (not a cargo crate, not embedded
  in the Rust binary) so the **distribution can use
  `npm install -g`** and the **install can use curl|bash**
  with the same artefact.
- New capability `control-menu` with typed domain objects:
  `MenuItem`, `CommandOutcome`, `CommandContext`,
  `ExecResult`. Pure types — no behaviour — so future
  implementers (and tests) share one shape.
- New CLI: `opctl` (canonical) and `openpanel-menu`
  (long-name alias). Both call the same entry point.
- New menu table with the seven actions the operators asked
  for, plus `0` for exit. `MENU_ITEMS` is `ReadonlyArray` so
  ids cannot drift at runtime, and the `handlers` map is built
  once at module load.
- New distribution path:
  - `npm install -g openpanel-menu` from the public registry
    (or the project mirror); `opctl` appears on `PATH`.
  - `curl -fsSL https://openpanel.dev/install.sh | bash`
    bootstraps Node.js 20+ via the system package manager
    (apt / dnf / pacman / apk) and then runs
    `npm install -g openpanel-menu`. The script is idempotent
    and is verified by `shellcheck` and a fixture-driven
    integration test that runs on Debian 12, Fedora 40,
    Arch, and Alpine 3.20.
- New tests covering:
  - unit: dispatch, validator, port parser, exec wrapper;
  - CLI E2E: the dispatcher, including masked input, on a
    fixture PTY;
  - shell: `install.sh` produces a working `opctl` on each
    of the four supported distros in CI.

## Capabilities

### New Capabilities

- `control-menu`: the menu table, dispatcher, command
  handlers, and distribution surface (`opctl` CLI entry,
  npm package, curl|bash installer).

## Impact

- New directory: `packages/openpanel-menu/` with `src/`,
  `test/`, `scripts/`, plus `package.json`, `tsconfig.json`,
  `README.md`, `.npmignore`. The package is **independent**
  from the Rust workspace and has its own
  `Cargo`-independent `Makefile` (a one-line wrapper around
  `npm`).
- New CI workflow `.github/workflows/control-menu.yml`
  running `npm install`, `npm run lint`, `npm test`, and a
  smoke `npm pack` on every push.
- New top-level `npm` script `scripts/dev-menu.sh` that runs
  the menu against a local copy so contributors can iterate
  without publishing.
- No changes to `crates/*`; `openspec/specs/quality` is
  modified in a follow-on change to opt this package into the
  File-length pipeline (which lands in the same sprint via
  `add-file-length-pipeline`).
