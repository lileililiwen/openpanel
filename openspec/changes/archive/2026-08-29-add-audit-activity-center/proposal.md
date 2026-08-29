# Add audit and activity center

## Why

The UI exposes security, deployment, backup, DNS, SSL, service, and user mutations, but `/audit` still returns HTTP 501. Operators cannot answer who changed a resource, what happened, whether a job failed, or which action caused an outage.

## What

Add an owner-only audit page and an activity summary surface. Provide filters for actor, action, target, time range, result, and correlation/job ID; paginate deterministically; redact secrets; link activity to resource pages and long-running jobs.

## Capabilities

### New

- Searchable audit event UI.
- JSON/API and HTML/HTMX representations with cursor pagination.
- Activity summaries for recent failures and in-progress jobs.

### Modified

- Dashboard and resource pages may show links to relevant activity.
- Navigation exposes Audit only to permitted roles.

## Non-goals

- No arbitrary log viewer replacement.
- No secret or request-body storage.
- No audit deletion from the UI.

## Dependencies

Depends on `repair-ui-discoverability` for navigation registration. May reuse existing audit storage without schema changes unless query indexes are proven necessary.
