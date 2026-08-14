# Add OpenPanel control menu CLI — Design

## Two artefacts, one package

```
packages/openpanel-menu/                       ← Backend (menu + commands)
├── package.json                                npm metadata
├── tsconfig.json                               strict ESM, Node 20
├── README.md
├── .npmignore
├── src/
│   ├── index.ts                  entry, calls runMenu()
│   ├── menu.ts                   interactive loop + dispatch
│   ├── menu-data.ts              MenuItem[], handlers map, renderHeader()
│   ├── lib/
│   │   ├── prompt.ts             readline + stty-echo helpers
│   │   ├── exec.ts               typed spawn wrapper (Promise<ExecResult>)
│   │   ├── stty.ts               child_process.execFile('stty')
│   │   ├── config.ts             hand-rolled port-line parser
│   │   └── log.ts                coloured console + greppable tags
│   └── commands/
│       ├── serve-start.ts        (3) systemctl start openpanel.service
│       ├── serve-stop.ts         (2) systemctl stop  + confirmation
│       ├── serve-restart.ts      (1) systemctl restart + post-probe
│       ├── change-port.ts        (4) edit /etc/openpanel/openpanel.toml
│       ├── change-password.ts    (5) openpanel user update-password
│       ├── show-info.ts          (6) --version + port + is-active
│       └── upgrade.ts            (7) cargo install + npm install -g
└── test/
    ├── menu.test.ts              dispatch + MENU_ITEMS table
    ├── prompt.test.ts            default value + confirm parsing
    ├── exec.test.ts              real spawn of `node --version`
    ├── config.test.ts            port-line parser
    └── password.test.ts          ≥12 chars + letter+digit rule

scripts/install.sh                              ← Distribution (curl|bash)
.github/workflows/control-menu.yml              CI for both parts
```

The backend and the distribution script live under the **same
package** because npm publish is the unit of distribution:
`scripts/install.sh` is referenced in `package.json#files`,
gets shipped to npm, and is hosted at the same URL that ships
the npm tarball.

## Menu contract

```
=========== OpenPanel 控制菜单 v0.1 ===========
(1) 重启面板         (2) 停止面板         (3) 启动面板
(4) 修改面板端口     (5) 修改管理员密码   (6) 查看面板信息
(7) 升级面板         (0) 退出
================================================
请输入命令编号:
```

- `0` is reserved for exit and is NOT a row in `MENU_ITEMS`.
- Adding a command means (a) adding a row to `MENU_ITEMS`,
  (b) writing `src/commands/<handler>.ts`, and (c) registering
  the module in `handlers`. Existing ids MUST NOT change once
  shipped — operators muscle-memory them.
- Out-of-range or non-numeric input prints
  `无效的命令编号,请重新输入.` and re-prompts. The loop never
  crashes on bad input.

## Command handlers — typed and testable

Each command module exports:

```ts
export async function run(ctx: CommandContext): Promise<CommandOutcome>;
```

`CommandContext` carries:

```ts
interface CommandContext {
  promptLine(msg, opts?): Promise<string>;
  promptHidden(msg): Promise<string>;       // masked with stty -echo
  promptConfirm(msg, defaultYes?): Promise<boolean>;
  log: Logger;                              // info / success / warn / error / raw
  exec: ExecLike;                           // systemctl / openpanel / run
}
```

This shape lets a unit test pass a stub context
(makeTestContext in `menu.ts`) instead of mocking every
helper. The full `runMenu` flow is exercised against a
fixture PTY in `tests/cli/`.

Concrete handler contracts:

| id | title             | implementation                                                     |
|----|-------------------|--------------------------------------------------------------------|
| 1  | 重启面板          | `systemctl cat openpanel.service` (precheck), `systemctl restart`, 1-s post-probe `is-active` |
| 2  | 停止面板          | promptConfirm (defaultNo), `systemctl stop`                       |
| 3  | 启动面板          | `is-active` precheck, `systemctl start`                            |
| 4  | 修改面板端口      | read current port (CLI preferred, file fallback), validate 1024-65535, `ss -ltn` reject-in-use, rewrite `/etc/openpanel/openpanel.toml` with backup, optional restart |
| 5  | 修改管理员密码    | masked prompt + confirm match, `openpanel user update-password --username X --password Y` |
| 6  | 查看面板信息      | `openpanel --version`, `cat /etc/openpanel/openpanel.toml` port, `systemctl is-active` |
| 7  | 升级面板          | sub-choice (cli / menu / both), `cargo install openpanel-cli --locked --force`, `npm install -g openpanel-menu@latest`, `systemctl restart openpanel` |

Idempotency rules:

- 1, 2, 3 each pre-check state so a no-op is reported as a
  no-op, not a second `restart`.
- 4 detects `next == current` and short-circuits.
- 5 rejects mismatched confirm; never writes a partial change.
- 7 is split into two discrete steps so a single failure does
  not leave the system in a half-upgraded state; the
  `systemctl restart` is the very last step.

## Secret hygiene

- Passwords are accepted through `promptHidden`, which uses
  `stty -echo`. Plaintext passwords are passed as **positional
  argv** to `openpanel user update-password`. They are NEVER
  on a shell command line (the exec wrapper uses
  `child_process.spawn` with `shell: false`).
- The confirm step is also masked; the variable is dropped
  immediately after the comparison, narrowing the window in
  which the plaintext is held in memory.
- Plaintext passwords do NOT appear in stdout, stderr, log
  tags, or audit markers — `Logger.error` redacts known
  secret shapes before printing.

## Distribution — npm

`package.json`:

```jsonc
{
  "name": "openpanel-menu",
  "version": "0.1.0",
  "license": "MIT",
  "type": "module",
  "main": "./dist/index.js",
  "bin": {
    "opctl": "./dist/index.js",
    "openpanel-menu": "./dist/index.js"
  },
  "files": ["dist", "scripts/install.sh", "README.md", "LICENSE"],
  "engines": { "node": ">=20" },
  "scripts": {
    "build":  "tsc -p tsconfig.json",
    "prepare":"npm run build",
    "start":  "node ./dist/index.js",
    "dev":    "tsx ./src/index.ts",
    "lint":   "tsc --noEmit -p tsconfig.json",
    "test":   "node --import tsx --test test/**/*.test.ts"
  },
  "devDependencies": {
    "@types/node": "^20.0.0",
    "tsx": "^4.7.0",
    "typescript": "^5.4.0"
  }
}
```

- `bin` exposes both `opctl` (canonical, short, six chars)
  and `openpanel-menu` (long alias) pointing at the same
  compiled `dist/index.js`.
- `files` is explicit: only the compiled `dist/`, the
  curl|bash installer, README, and LICENSE ship. Source
  TypeScript, tests, dev config, and `node_modules` are
  excluded (the `.npmignore` is belt-and-braces).
- `engines.node` is `>=20` because TypeScript 5 + native
  `node:test` and `node:readline/promises` need 18+, and the
  raw-key handling for `stty -echo` is documented to behave
  correctly on 20+.
- `prepare` runs `npm run build` so a `npm install` (or
  `npm install -g`) from the registry compiles the package
  before exposing the bin.
- No runtime dependencies. Everything is on `node:*` standard
  library so the package size is small and the supply chain
  is reduced to npm itself.

## Distribution — curl | bash

`scripts/install.sh` is the single-file script referenced by
the published npm tarball. The flow:

```
┌──────────────────────────────────────────────────────────────────┐
│ curl -fsSL https://openpanel.dev/install.sh | bash                │
└──────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
            ┌──────────────────────────────────────┐
            │ 1. /etc/os-release → DISTRO_ID       │
            │    (ubuntu / debian / fedora /        │
            │     rhel / centos / rocky / almalinux │
            │     / arch / manjaro / alpine / other)│
            └──────────────────────────────────────┘
                                  │
                                  ▼
            ┌──────────────────────────────────────┐
            │ 2. ensure node ≥20                    │
            │    on PATH: skip                      │
            │    apt  : setup_X.x + apt install     │
            │    dnf  : setup_X.x + dnf install     │
            │    pacman: -Sy nodejs npm             │
            │    apk  : apk add nodejs npm          │
            │    other: bail w/ EX_CONFIG + URL      │
            └──────────────────────────────────────┘
                                  │
                                  ▼
            ┌──────────────────────────────────────┐
            │ 3. `npm install -g openpanel-menu`   │
            │    via sudo if not root               │
            └──────────────────────────────────────┘
                                  │
                                  ▼
            ┌──────────────────────────────────────┐
            │ 4. `opctl --version` smoke           │
            │    print next-step hint               │
            └──────────────────────────────────────┘
```

Hardening rules (mirroring what `openspec/specs/quality/spec.md`
expects of any installer on this panel):

- `set -euo pipefail` at the top.
- Detects root: `id -u == 0` means `SUDO=""`; otherwise
  `SUDO=sudo`; bail with `exit 78` if `sudo` is unavailable.
- The `node_major_version` helper is a single shell function
  tested under both `bash` and `dash`.
- The four distro branches are each guarded by a sub-test in
  CI (Docker images `debian:12`, `fedora:40`, `archlinux:latest`,
  `alpine:3.20`); branches fall through to a clear error
  pointing at https://nodejs.org/en/download.
- The script is `shellcheck clean` (`shellcheck -x` — sourced
  files are not in scope here, so no exceptions).
- Idempotent: re-running the script never doubles up; it
  either says "Node.js vX.Y.Z 已安装,无需再装." or upgrades.

## Test strategy

```
1.1  Unit (test/menu.test.ts):
     - readChoice handles 0 / in-range / out-of-range / non-
       numeric / whitespace / empty
     - MENU_ITEMS excludes id 0
     - renderHeader contains every menu row
1.2  Unit (test/prompt.test.ts):
     - default value applied on empty
     - confirm('y') → true, confirm('n') → false
     - non-masking fallback for non-TTY
1.3  Unit (test/exec.test.ts):
     - real `node --version` exits 0, stdout matches /^v\d/
     - non-zero exit codes are reported via ok=false (no throw)
     - missing binary throws ExecError
     - stderr captured separately from stdout
1.4  Unit (test/config.test.ts):
     - parsePort simple / TOML / case-insensitive / range /
       comments / whitespace / missing
1.5  Unit (test/password.test.ts):
     - 12-char boundary, 11-char reject, no-digit reject,
       no-letter reject, accepted strong password
1.6  Service tests (test/handlers.test.ts, with stub context):
     - serve-start short-circuits when is-active = active
     - serve-stop requires confirmation
     - serve-restart pre-checks unit existence
     - change-port rejects port < 1024 or > 65535
     - change-password rejects short / mismatched
1.7  CLI E2E (tests/cli/menu.test.ts):
     - read choice 0 exits 0
     - read choice "abc" re-prompts with the standard message
     - masked prompt hides input (PTY-driven)
1.8  Shell E2E (tests/install/):
     - install.sh on debian:12 succeeds with Node already 20+
     - install.sh on fedora:40 upgrades an older Node
     - install.sh on alpine:3.20 uses apk
     - install.sh reports EX_CONFIG on a fake unsupported distro
```

## Risks and mitigations

| Risk                                                        | Mitigation                                                                                       |
|-------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| `stty -echo` blocked in containers / WSL                    | `promptHidden` falls back to plain readline with a visible warning; the canonical path requires a TTY |
| Operator runs `opctl` over SSH without `-t`                 | Plain readline works; masking falls through; the spec calls this acceptable                     |
| `openpanel` CLI not on PATH on a developer's laptop         | Each command checks exit code; on failure prints a `command not found`-style hint instead of crashing |
| Curl|bash runs scripts as root by accident                  | `need_root` reads `id -u`; warns if `sudo` is missing when not root                              |
| npm install on a networkless box                            | `install.sh` writes a clear "Node found but npm install failed" message; the spec does not require the panel to support offline installs in v0.1 |
| Menu ids change between releases                            | Lint rule from `add-file-length-pipeline` plus a unit test that snapshots `MENU_ITEMS` is added in a follow-up |
| `change-password` argument list visible in `ps`             | argv is passed via `spawn`'s argv array; no `shell: true`; the spec calls this a hard rule         |

## Out of scope for this change

- Web UI for the same controls (covered by existing
  `web-ui` capability).
- Generic Linux sysadmin commands (nginx, mysql, redis, docker)
  directly — those are out of scope per the pre-change
  conversation; the menu is OpenPanel-specific.
- i18n beyond Chinese + English. English messages are emitted
  on every menu line in v0.1; full localisation lands in the
  `add-i18n-and-localization` change.
- Plugin / extension framework for the menu. The
  `add-plugin-extension-framework` change will let third-party
  packages add menu items in a follow-on release.
