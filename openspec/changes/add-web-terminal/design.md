# Add Web terminal — Design

## Explore & Reuse

- **Auth extractors**: `AuthSession` / `RequireRole`
  (`crates/openpanel-api/src/extract/`) gate the ticket endpoint;
  `session_middleware` pattern reused for the WS route's pre-upgrade
  auth.
- **axum ws**: workspace already enables axum feature `"ws"`
  (`Cargo.toml:33`) — no new web framework dep.
- **Jail precedent**: `openspec/specs/sftp-jailed-shells/spec.md`
  defines per-site users + ownership boundaries; the PTY spawns as that
  same user so file ownership rules stay intact.
- **Background tasks**: `Module::background_tasks(&AppContext)`
  (core `module.rs`) hosts the idle-session reaper.
- **Audit**: `AuditService` events `TerminalOpened{user, site}`,
  `TerminalClosed{duration}` — no content capture.
- **Test doubles**: `openpanel-test-support` gains `MockPty` implementing
  `PtyPort`; WS tests use `TestServer`.

## Ticket flow

```
POST /api/v1/terminal/ticket {site_id}   (auth: Owner/Admin, CSRF)
  -> TerminalTicket{token: 256-bit random, ttl = 30s, single_use}
WS  /api/v1/terminal/session?ticket=...   (Origin must match panel host)
  consume(ticket): unknown|used|expired -> close(1008)
  ok -> spawn PtyPort.open(user=site_user, cwd=site_root, env=minimal)
        bridge ws <-> pty with backpressure cap (e.g. 1 MiB buffered,
        drop policy: close session on overflow)
```

Domain owns the pure ticket state machine:

```rust
pub struct TerminalTicket { token: Token, expires_at, consumed: bool }
impl TerminalTicket {
    pub fn new(now, ttl) -> Self;              // random via injected rng port
    pub fn consume(self, now) -> Result<Token, TerminalError>; // Used | Expired
}
pub struct TerminalSession { id, user_id, site_id, opened_at, closed_at: Option<_> }
```

## PTY port

```rust
pub trait PtyPort: Send + Sync {
    async fn open(&self, spec: PtySpec) -> Result<Box<dyn PtyStream>, TerminalError>;
}
pub struct PtySpec { user: String, cwd: PathBuf, shell: String, term: String }
```

App-layer impl uses `portable-pty` (new workspace dep, app layer only)
dropping privileges to the site user; `unsafe_code = "forbid"` holds —
the crate internally manages the tty, we only call its safe API.

## Limits & config

```toml
[web_terminal]
enabled            = true
idle_timeout_secs  = 300
max_sessions_per_user = 1
output_buffer_bytes   = 1048576
```

Merged through the standard config layering; disabled → routes return
404 (feature off leaves no surface).

## Layering

Domain: tickets/sessions + traits (no tokio). App: service, repo,
PtyPort impl, reaper task. API: ticket POST + WS upgrade route. CLI:
ticket printing. Web: terminal tab. Composition root registers
`WebTerminalModule`.
