# control-menu Specification

## Purpose
TBD - created by archiving change 2026-08-13-add-control-menu-cli. Update Purpose after archive.
## Requirements
### Requirement: Menu Table Is Stable

The system SHALL expose a `MENU_ITEMS` table whose rows are
in the order (1) restart panel, (2) stop panel, (3) start
panel, (4) change panel port, (5) change admin password,
(6) show info, (7) upgrade. The numeric id `0` SHALL be
reserved for exit and SHALL NOT appear in `MENU_ITEMS`. The
table MUST be `ReadonlyArray<MenuItem>` and the handler map
`handlers: ReadonlyMap<number, Handler>` so ids cannot drift
at runtime. Once a version ships, ids SHALL NOT renumber in
later releases.

#### Scenario: Reserved exit id

- **WHEN** a developer reads `MENU_ITEMS`
- **THEN** no row has id `0`; the value `0` is reserved in `EXIT_ID`.

#### Scenario: Handlers cover every menu id

- **WHEN** the panel builds `handlers`
- **THEN** every id in `MENU_ITEMS` has exactly one entry in `handlers`; ids without a row return "无效的命令编号" without consulting the map.

#### Scenario: Id stability between versions

- **WHEN** an operator who learned the menu in v0.1 upgrades to v0.2
- **THEN** the same id still means the same action; new entries MAY be appended after id `7` but MUST NOT replace existing rows.

### Requirement: Two-Column Menu Rendering

`renderHeader(version)` SHALL emit a banner with the version,
the seven command rows, and the exit row, formatted as
two columns of three entries separated by whitespace, mirroring
Baota's visual layout. The banner is the only string the
operator sees before the first prompt.

#### Scenario: Banner contains every menu row

- **WHEN** `renderHeader('0.1.0')` returns
- **THEN** the string contains `(1) 重启面板`, `(2) 停止面板`, `(3) 启动面板`, `(4) 修改面板端口`, `(5) 修改管理员密码`, `(6) 查看面板信息`, `(7) 升级面板`, and `(0) 退出`; the version appears in the title.

### Requirement: Dispatcher Loop Resilient To Bad Input

`runMenu()` SHALL prompt for a choice, dispatch to the
handler registered under that id, render the handler outcome,
and re-prompt. Out-of-range or non-numeric input SHALL NOT
crash; the loop SHALL print `无效的命令编号,请重新输入.` and
continue. Whitespace around the typed digit SHALL be ignored
(full-width U+3000 and ASCII spaces both stripped).

#### Scenario: Out-of-range input re-prompts

- **WHEN** the operator types `99`
- **THEN** the dispatcher prints the standard message and re-prompts; the menu stays open.

#### Scenario: Non-numeric input re-prompts

- **WHEN** the operator types `abc`
- **THEN** the dispatcher prints the standard message and re-prompts.

#### Scenario: Whitespace tolerated

- **WHEN** the operator types `  \u3000 1 \u3000  `
- **THEN** the dispatcher treats the input as id `1`.

#### Scenario: Exit id ends the session

- **WHEN** the operator types `0`
- **THEN** the dispatcher prints `再见.` and the menu exits with code `0`.

### Requirement: Typed Command Handlers

Every command module SHALL export
`export async function run(ctx: CommandContext): Promise<CommandOutcome>`
where `CommandContext` carries `promptLine`, `promptHidden`,
`promptConfirm`, `log`, and `exec`. The shape MUST be sufficient
for hermetic unit tests via the `makeTestContext` helper;
handlers MUST NOT import readline, `process.stdin`, or
`child_process` directly.

#### Scenario: Handlers accept the shared context

- **WHEN** a unit test calls `serveRestart.run(makeTestContext({ ... }))`
- **THEN** the handler uses the stub `ctx.exec.systemctl` and the stub `ctx.promptConfirm` exactly as configured; no real subprocess runs.

#### Scenario: Outcome reports continue or exit

- **WHEN** a handler completes
- **THEN** it returns `{ kind: 'continue' }` to re-prompt, or `{ kind: 'exit' }` to end the session; no other outcome shape is allowed.

### Requirement: serve-start Idempotency

`serve-start` SHALL pre-check `systemctl is-active openpanel.service`
and, when it returns `active`, print `openpanel 服务已在运行 — 无需重复启动.`
without issuing a redundant `start`. The precheck failure
mode (systemctl missing, unit not registered) is reported as
a warning + remediation and the menu re-prompts.

#### Scenario: Already active

- **WHEN** the panel's systemd unit is already `active` and the operator chooses `3`
- **THEN** the command prints the no-op message and the menu re-prompts; no second `start` is issued.

#### Scenario: Unit missing

- **WHEN** `systemctl cat openpanel.service` fails
- **THEN** the command prints the unit-not-found message and the menu re-prompts.

### Requirement: serve-stop Requires Confirmation

`serve-stop` SHALL ask `promptConfirm('确定要停止 openpanel 服务吗?', false)`
before issuing `systemctl stop openpanel.service`. A `false`
answer re-prompts the menu without touching the unit.

#### Scenario: User cancels

- **WHEN** the operator picks `2` and answers `n` to the confirm
- **THEN** the command prints `已取消.` and the menu re-prompts; `systemctl stop` is not invoked.

#### Scenario: User confirms

- **WHEN** the operator picks `2` and answers `y`
- **THEN** the command issues `systemctl stop openpanel.service`; on success prints `openpanel 服务已停止.`; on failure prints the redacted error and re-prompts.

### Requirement: serve-restart Pre-Checks The Unit

`serve-restart` SHALL verify the unit file is registered
(`systemctl cat openpanel.service`) BEFORE issuing
`systemctl restart openpanel.service`. After a successful
restart, the command SHALL sleep 1 second and probe
`is-active` to confirm the new process is healthy. A unit
that is missing, a restart that fails, or a post-probe that
returns non-active all re-prompt the menu with a typed
remediation message.

#### Scenario: Unit missing

- **WHEN** the panel's systemd unit is not registered
- **THEN** the command prints `未发现 openpanel.service 单元文件.` and the menu re-prompts; no restart is attempted.

#### Scenario: Successful restart

- **WHEN** the restart command exits 0 and the post-probe returns `active`
- **THEN** the command prints `openpanel 服务已重启.` and the menu re-prompts.

#### Scenario: Restart fails

- **WHEN** `systemctl restart openpanel.service` exits non-zero
- **THEN** the command prints the redacted stderr and the menu re-prompts.

### Requirement: change-port Validates And Restarts

`change-port` SHALL read the current port, prompt for a new
port, validate the new port (1 ≤ N ≤ 65535, not in use per
`ss -ltn`), back up `/etc/openpanel/openpanel.toml` next to
itself as `<file>.opctl.bak`, rewrite the `port = NNNN` line,
and ask whether to restart. When restart is accepted, the
service restarts; on failure the operator is shown a roll-back
instruction (`cp <file>.opctl.bak <file> && systemctl restart`).

#### Scenario: Same port is a no-op

- **WHEN** the operator enters the same port as the current one
- **THEN** the command prints `新端口与当前相同 — 无需修改.` and the menu re-prompts without writing.

#### Scenario: Out-of-range port rejected

- **WHEN** the operator enters `80` or `70000`
- **THEN** the command prints `端口号必须是 1024-65535 之间的整数.` and the menu re-prompts.

#### Scenario: Port in use rejected

- **WHEN** `ss -ltn` shows the proposed port already bound
- **THEN** the command prints `端口 N 已被其他进程占用.` and the menu re-prompts.

#### Scenario: Successful change

- **WHEN** the new port passes validation and the operator accepts the restart
- **THEN** the config file is rewritten atomically (`copyFile → writeFile`), the unit restarts, and the menu re-prompts.

#### Scenario: Restart fails

- **WHEN** the rewrite succeeds but `systemctl restart` fails
- **THEN** the command prints the roll-back hint and the menu re-prompts.

### Requirement: change-password Enforces Panel Policy

`change-password` SHALL read the username with a default of
`admin`, read a new password via `promptHidden`, validate
that the plaintext is ≥ 12 chars and contains at least one
letter and one digit, ask for a confirmation masked prompt,
reject mismatched input, and pass the plaintext to
`openpanel user update-password --username X --password Y`
via `spawn`'s argv array (NEVER on a shell command line). The
plaintext MUST NOT appear in stdout, stderr, audit tags, or
`ps` output.

#### Scenario: Weak password rejected

- **WHEN** the operator enters a 6-character password
- **THEN** the command prints `密码必须 ≥ 12 字符,且至少包含一个字母和一个数字.` and the menu re-prompts; the CLI is NOT invoked.

#### Scenario: Mismatched confirm rejected

- **WHEN** the operator enters two values that differ
- **THEN** the command prints `两次输入不一致.` and the menu re-prompts.

#### Scenario: Successful rotation

- **WHEN** the password passes validation and matches confirmation
- **THEN** `openpanel user update-password --username <u> --password <p>` runs and the command prints `用户 <u> 的密码已更新.` followed by `请将新密码记在安全的密码管理器中.`

#### Scenario: Plaintext never visible

- **WHEN** `ps -ef` is queried during a `change-password` run
- **THEN** the command line of the `openpanel user update-password` process does NOT contain the plaintext password; only the structural argv `--password <p>` is observable, where `<p>` is the actual plaintext passed via spawn and not echoed to the menu.

### Requirement: show-info Is Read-Only

`show-info` SHALL print `openpanel --version`, the configured
port from `/etc/openpanel/openpanel.toml`, and the systemd
state from `systemctl is-active openpanel.service`. The
command SHALL NOT mutate any state.

#### Scenario: Working panel

- **WHEN** the operator picks `6` on a panel running v0.1 with port 8080 active
- **THEN** the menu renders `版本: v0.1.0 / 监听端口: 8080 / systemd 状态: active` and the menu re-prompts.

#### Scenario: Panel not running

- **WHEN** the service is inactive
- **THEN** the line reads `systemd 状态: inactive/dead` and the menu re-prompts.

### Requirement: upgrade Splits Sub-Actions

`upgrade` SHALL ask which target to upgrade (`1=cli`, `2=menu`,
`3=both`), confirm with the operator, run the corresponding
`cargo install` and/or `npm install -g` step, then restart
the service so the new binary is in use. Each sub-step fails
independently without rolling back the other.

#### Scenario: Upgrade cli only

- **WHEN** the operator chooses `1` and confirms
- **THEN** `cargo install openpanel-cli --locked --force` runs; on success the menu prints success and `systemctl restart openpanel.service` runs.

#### Scenario: Upgrade menu only

- **WHEN** the operator chooses `2` and confirms
- **THEN** `npm install -g openpanel-menu@latest` runs; on success the menu prints success and the service restarts.

#### Scenario: Upgrade both

- **WHEN** the operator chooses `3`
- **THEN** both installs run sequentially, each reporting its own outcome; the service restart is the very last step.

#### Scenario: User cancels

- **WHEN** the operator picks `7` and answers `n` to the confirm
- **THEN** the command prints `已取消.` and the menu re-prompts; no install runs.

### Requirement: npm Distribution

The package SHALL be published to npm as `openpanel-menu`
with the bin entries `opctl` and `openpanel-menu` both
pointing at `dist/index.js`. The `files` field SHALL be
explicit: `dist/`, `scripts/install.sh`, `README.md`,
`LICENSE`. The `engines.node` field SHALL be `>=20`. The
`type` field SHALL be `module`. The `prepare` script SHALL
compile TypeScript before the package becomes globally
runnable so `npm install -g` (from a registry or a git URL)
produces a working `opctl`.

#### Scenario: npm install -g makes opctl reachable

- **WHEN** `npm install -g openpanel-menu` runs on a machine with Node 20+
- **THEN** `opctl --version` is on `PATH` and exits 0; `openpanel-menu` is an alias to the same binary.

#### Scenario: tarball size budget

- **WHEN** `npm pack` produces the tarball
- **THEN** the tarball is at most 1 MiB; the file list exactly matches `package.json#files` plus `package.json`.

#### Scenario: Engines enforces Node 20

- **WHEN** `npm install -g` is invoked on Node 18
- **THEN** `npm` prints a warning explaining the engine requirement and refuses to install.

### Requirement: curl|bash Distribution

The `scripts/install.sh` script SHALL be hostable at
`https://openpanel.dev/install.sh` and SHALL (a) detect
`/etc/os-release` for the distro family, (b) install Node 20+
via the matching package manager (apt / dnf / pacman / apk),
(c) `npm install -g openpanel-menu`, (d) smoke-test
`opctl --version`, and (e) print next-steps. The script MUST
NOT prompt for user input; it MUST be idempotent and silent
on partial failure (logs to stderr, exit code reflects
outcome).

#### Scenario: Debian 12 with no Node

- **WHEN** the script runs inside a clean `debian:12` container as root
- **THEN** it installs `curl`, fetches NodeSource's setup script, installs `nodejs`, runs `npm install -g openpanel-menu`, ends with `opctl --version` exiting 0.

#### Scenario: Fedora 40 with Node 18

- **WHEN** the script runs on Fedora 40 with Node 18 installed
- **THEN** it detects the major version, upgrades via NodeSource's RPM repo, and ends with Node 20+ installed.

#### Scenario: Alpine 3.20

- **WHEN** the script runs on Alpine 3.20
- **THEN** `apk add --no-cache nodejs npm` runs and the menu is reachable afterwards.

#### Scenario: Unsupported distro

- **WHEN** `/etc/os-release` reports an `ID` the script does not recognise
- **THEN** the script prints an error pointing the operator at https://nodejs.org/en/download and exits with code `78` (EX_CONFIG).

#### Scenario: Sudo missing for non-root

- **WHEN** the script is piped into bash from a non-root user and `sudo` is unavailable
- **THEN** the script prints `安装包需要 root 权限,但 sudo 不可用. 请以 root 重新运行此脚本.` and exits non-zero; nothing is installed.

### Requirement: Secret Hygiene

The exec wrapper (`src/lib/exec.ts`) SHALL spawn subprocesses
with `shell: false` and an argv array. The `openpanel user
update-password` subprocess SHALL receive `--password
<plaintext>` via argv and SHALL NOT receive the plaintext on
the command line via shell parsing. The masked prompt helper
`promptHidden` SHALL use `stty -echo` on a real TTY and SHALL
fall back to a visible plain read with a warning on a
non-TTY.

#### Scenario: TTY masking

- **WHEN** the operator runs `opctl` in a real terminal and chooses `5`
- **THEN** the typed password does not appear on screen; each byte is replaced with `*`; the panel's CLI still receives the plaintext via argv.

#### Scenario: No-TTY fallback

- **WHEN** `opctl` is run without a TTY (e.g. CI)
- **THEN** `promptHidden` falls back to plain read; a warning line `警告: 无法屏蔽输入回显. 密码将以明文显示.` is printed.

#### Scenario: ps never shows the plaintext on a separate exec

- **WHEN** `ps -ef` is captured during a `change-password` run
- **THEN** the only `--password <value>` occurrence is the spawn argv of the child; the menu's own log tags never include the value.

### Requirement: Audit and Logging

The `Logger` SHALL emit colour-coded, greppable tags
(`[INFO] [OK] [WARN] [ERROR]`) prepended to every message. On a
non-TTY the colour escapes are stripped. No logger line may
include a plaintext password, an unredacted token, the
contents of a `/etc/` file, or the value of `--password`.

#### Scenario: Greppable tag

- **WHEN** the operator captures `opctl 2>&1 | grep '\[OK\]'`
- **THEN** only the success lines are returned; status tags survive piping.

#### Scenario: Non-TTY colour strip

- **WHEN** `opctl` output is captured to a file
- **THEN** the file contains no ANSI escape sequences; only the message text and the bracketed tag.

### Requirement: Tests Live In The Package

The package SHALL ship tests under `test/` using
`node:test` with the `tsx` loader. Every public function in
`src/` SHALL have at least one unit test; every command
module SHALL have at least one service test driven by
`makeTestContext`; the full install path SHALL have a
Docker-matrix shell E2E in `tests/install/`.

#### Scenario: Unit test suite green

- **WHEN** `cd packages/openpanel-menu && npm test` runs
- **THEN** every test passes without network access, without root, and without an `openpanel` binary on `PATH`.

#### Scenario: Service tests run without spawning

- **WHEN** `npm test` runs the handler suite
- **THEN** each handler is invoked with a stub `ctx.exec` so no real `systemctl`, `npm`, or `cargo` invocation happens.

#### Scenario: Docker-matrix install test

- **WHEN** GitHub Actions runs the `control-menu` workflow on a PR
- **THEN** four jobs (`debian-12`, `fedora-40`, `archlinux`, `alpine-3.20`) each install the script and verify `opctl --version`; a fifth job verifies the EX_CONFIG path on a fake distro.

