# Add AI Ops agent — Design

## AiAction model

```rust
pub struct AiAction {
    pub session_id: AiSessionId,
    pub tool: ToolName,          // must exist in ToolCallAllowlist
    pub kind: ToolKind,          // Read | Write
    pub params: serde_json::Value,
    pub status: AiActionStatus,  // Proposed | Approved | Executed | Denied
    pub approved_by: Option<UserId>,
    pub audit_id: Option<AuditId>,
}
```

## Tool allowlist

```rust
pub struct ToolCallAllowlist {
    pub tools: Vec<ToolSpec>, // { name, kind: Read | Write, schema }
}
// The executor resolves a tool ONLY if present; otherwise it is refused.
// No tool may spawn a shell; each tool delegates to an existing service.
```

## Ask / act flow

```
ask(session_id, prompt):
  resolve tools from prompt (read-only candidates)
  for read tool: execute via ToolExecutor, attach result
  for write tool: create AiAction{status=Proposed}, await approval
  compose answer; persist AiMessage; audit AiAsked

approve(action_id, principal):
  require principal authorised by identity + audit
  set status=Approved, approved_by=principal
  execute via ToolExecutor; set status=Executed; audit AiActionExecuted

deny(action_id, principal):
  set status=Denied; audit AiActionDenied
```

## Endpoints

```
POST /api/v1/ai/ask            body { session_id?, prompt }
GET  /api/v1/ai/sessions       query { limit? }
POST /api/v1/ai/actions        body { action_id, decision: approve|deny }
```

## Tests

```
1.1 Unit: allowlist lookup (present/absent); kind tagging read/write.
1.2 Property: no tool execution escapes the allowlist; write action
    cannot execute without an approval record.
1.3 Service tests w/ mock services: ask read-only, approve write,
    deny write.
1.4 Integration: bad tool refused; approved write executes + audits;
    denied write never executes.
1.5 Web: AI panel (CSRF), ask box, pending-approval queue.
```
