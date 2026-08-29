# Design: Audit and activity center

## Explore & Reuse

- Reuse `openpanel_core::AuditService`, `AuditEvent`, `AuditAction`, and existing audit repositories.
- Reuse `AuthUser`/role extractors, `csrf_field`, `ui_states`, `layer` toasts, and table CSS.
- Reuse existing job IDs and status views from backups, software, previews, cron, and monitoring.
- Inspect `tests/integration/web_ui_audit.rs` before changing its current 501 contract; replace stub assertions with behavior assertions.

## Data contract

Expose metadata only: event ID, timestamp, actor display identifier, action, target type/name, outcome, safe metadata keys, and correlation ID. Apply a central redaction allowlist before rendering or serializing. Use cursor pagination ordered by timestamp descending then ID descending. Invalid filters return a visible 422 form error, not an empty result.

## UX

The page starts with an activity summary, then filter controls, then a responsive table. Each row has a detail disclosure with safe metadata and links. Empty, no-result, loading, error, and unavailable states use `ui_states`. Long-running activity links return to the owning job page.

## Security

Owner-only access is enforced server-side. Secrets, tokens, passwords, PEM material, request bodies, and arbitrary metadata values are never rendered. Reads are not audited; export and filter operations are read-only.

## Verification

Unit tests cover redaction, stable cursor ordering, and filter parsing. Integration tests cover unauthenticated redirect, user denial, owner filtering, pagination, no-result state, secret non-leakage, and HTMX fragment responses.
