# Add Web terminal

## Why

A browser terminal is de-facto baseline: cPanel (Terminal), Plesk,
HestiaCP 1.9+, Virtualmin, aaPanel/BaoTa, 1Panel, Coolify and Easypanel
all ship one. OpenPanel has none — the only trace is a doc-comment
label in the software-center recipe list. Users must fall back to raw
SSH, which the panel cannot scope, audit, or revoke per site.

## What Changes

- One-time, short-TTL **terminal tickets** issued to authenticated
  Owner/Admin sessions; consumed by a single WebSocket upgrade.
- **PTY session** spawned as the target site's jailed user (same
  ownership boundaries as SFTP jails) with minimal env; never root.
- **Guardrails**: Origin check on upgrade, idle timeout reaper,
  per-user concurrent-session cap, bounded output buffer with
  close-on-overflow, feature flag defaulting off in config.
- **Audit**: open/close events with user + site + duration; keystrokes
  and output are never logged.
- Surfaces: `POST /api/v1/terminal/ticket`, WS
  `/api/v1/terminal/session`, CLI `openpanel terminal ticket`, web
  Terminal tab.

## Capabilities

### New Capabilities

- `web-terminal`: audited, scoped browser terminal over WebSocket.

## Impact

- Domain: `TerminalTicket`, `TerminalSession`, `PtyPort` trait,
  `TerminalError` under `crates/openpanel-domain/src/web_terminal/`.
- App: `WebTerminalService`, SQLite ticket/session repo, `portable-pty`
  adapter (app layer only), idle-reaper background task.
- API: ticket route + WS route; CLI dispatch; web Terminal page.
- Security: no secrets in URLs after handshake (ticket is single-use,
  30 s TTL); output never persisted; disabled by default for
  suspended accounts.
- Coupling: identity (sessions/RBAC), sftp-jailed-shells (user
  mapping), core Module/background-task infra.

## Non-goals

- No session recording/replay (compliance change may add later).
- No multi-user shared/ro-terminal.
- No SFTP-in-browser (file manager already covers uploads).
