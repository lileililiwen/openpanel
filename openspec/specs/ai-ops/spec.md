# ai-ops Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-ai-ops-agent. Update Purpose after archive.
## Requirements
### Requirement: Ask Questions

The system SHALL let an authorised caller ask operational questions in
an `AiSession` and SHALL answer using read-only tool calls resolved
from a fixed allowlist. Read-only tool calls SHALL execute
automatically and SHALL be audited. The agent SHALL NOT execute any
tool absent from the allowlist, and SHALL NOT execute a shell.

#### Scenario: Read-only answer

- **WHEN** an authorised caller posts `POST /ai/ask` with a question
        resolvable to read-only tools
- **THEN** the answer is returned with attached tool results and an
        audit `AiAsked{session_id}` is recorded.

#### Scenario: Unknown tool refused

- **WHEN** a prompt resolves to a tool not present in the allowlist
- **THEN** the tool is refused, no execution occurs, and the response
        states the tool is not permitted.

### Requirement: Approve Write Actions

`POST /ai/actions` SHALL gate every `Write`-kind tool call behind an
explicit approval by an authorised principal. The system SHALL NOT
execute a write action whose status is not `Approved`, and SHALL record
`approved_by` and an audit `AiActionExecuted{action_id}` on execution.

#### Scenario: Approved write executes

- **WHEN** a principal approves a proposed write action
- **THEN** the action executes via the allowlisted tool, status
        becomes `Executed`, and an audit entry records the approver.

#### Scenario: Denied write never executes

- **WHEN** a principal denies a proposed write action
- **THEN** status becomes `Denied`, no execution occurs, and audit
        `AiActionDenied{action_id}` is recorded.

### Requirement: Audit Trail

The system SHALL record every question, tool call, approval decision,
and executed action under the `audit` context, including the agent
principal and the human approver where applicable. No raw command text
that could bypass the allowlist SHALL be persisted.

#### Scenario: Full trail

- **WHEN** a session asks, proposes a write, and that write is approved
- **THEN** `AiAsked`, the proposed `AiAction`, `AiActionExecuted`, and
        the approver are all present in the audit log for the session.

