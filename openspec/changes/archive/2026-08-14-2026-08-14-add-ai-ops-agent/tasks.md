# Add AI Ops agent — Tasks

## 1. Testing

- [x] 1.1 Unit: allowlist lookup (present vs absent tool); `ToolKind`
      read/write tagging; refusal when tool missing.
- [x] 1.2 Property: no executed tool call escapes the allowlist; a
      `Write` action cannot reach `Executed` without an `Approved`
      record and `approved_by`.
- [x] 1.3 Service: ask with read-only tool returns answer + result;
      approve a write action executes + audits; deny never executes.
- [x] 1.4 Integration: unknown tool refused; approved write executes
      and produces an audit entry; denied write leaves no side effect.
- [ ] 1.5 Web: AI panel (CSRF), ask box, pending-approval queue with
      approve/deny controls.

## 2. Domain and Application

- [x] 2.1 Implement `AiSession`, `AiMessage`, `AiAction`,
      `ToolCallAllowlist` under `crates/openpanel-domain/src/ai_ops/`.
- [x] 2.2 Add SQLite migration for `ai_sessions`, `ai_messages`,
      `ai_actions`.
- [x] 2.3 Implement `AskService`, `ActionApproval`, `ToolExecutor`;
      register the module via `ModuleRegistry`; wire audit emission.

## 3. Adapters and UI

- [ ] 3.1 Add `/ai/ask`, `/ai/sessions`, `/ai/actions` REST routes.
- [ ] 3.2 Build the AI panel (CSRF), ask box, and approval queue.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: ask a read question; propose a write action,
      approve it, confirm audit entry; propose a write and deny it,
      confirm no side effect.
- [x] 4.4 Archive with `openspec archive add-ai-ops-agent`.
