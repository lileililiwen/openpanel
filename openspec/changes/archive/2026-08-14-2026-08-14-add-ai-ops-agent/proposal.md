# Add AI Ops agent

## Why

Modern panels are shipping embedded AI ops assistants — 1Panel ships
OpenClaw, an agent that answers operational questions and can take
scoped remediation actions. OpenPanel has no equivalent surface, so
operators fall back to manual CLI work, grep through logs, and run
ad-hoc scripts with no trail. This change adds an `ai-ops` bounded
context: a conversational surface that answers ops questions and can
run **scoped, audited remediation** against the panel. The agent is a
read-mostly tool: it may read panel state freely, but any write action
must come from a fixed allowlist and require explicit human approval.

## What Changes

- New bounded context `ai-ops` carrying the `AiSession`, `AiMessage`,
  and `AiAction` aggregates, plus `AskService`, `ActionApproval`.
- New endpoints: `POST /api/v1/ai/ask`, `GET /api/v1/ai/sessions`,
  and `POST /api/v1/ai/actions` (approval gate).
- An agent surface that answers ops questions (read-only tool calls)
  and proposes remediation. Read-only tool calls run automatically;
  write tool calls are queued for approval.
- A tool-call **allowlist** enumerating every callable tool, each
  tagged `read` or `write`. The allowlist is the only surface the
  agent may touch — there is NO raw shell execution.
- Full audit trail: every question, tool call, approval, and executed
  action is recorded under the `audit` context.

## Capabilities

### New Capabilities

- `ai-ops`: a conversational agent that answers operational questions
  and runs scoped, audited remediation actions (read-only and approved
  writes) against the panel via a fixed tool-call allowlist.

## Impact

- Domain: `AiSession`, `AiMessage`, `AiAction`, `ToolCallAllowlist`.
- App: `AskService`, `ActionApproval`, `ToolExecutor`.
- API/CLI/web: `/ai/ask`, `/ai/sessions`, `/ai/actions`; web AI panel
  (CSRF).
- Security: no raw shell exec; allowlist enumerates every tool;
  write actions require explicit approval before execution; every
  action audited under `audit`. The agent identity is a least-privilege
  principal distinct from human roles.
- Coupling: depends on `api` for routing; emits audit events into
  `audit` (see `refine-web-ui-with-audit-accessibility-theming`);
  relies on `identity` for the approving principal and for
  authorisation of the agent surface.
