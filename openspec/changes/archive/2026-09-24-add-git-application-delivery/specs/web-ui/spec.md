# web-ui Specification

## Purpose

Web UI discoverability, accessibility, and theme contract. The
application-delivery change adds a new section of the panel
that lists applications, environments, releases, and rollback
state. The new section reuses the existing role gating and
theme tokens.

## Requirements

## ADDED Requirements

### Requirement: Application Delivery Web UI Reuses Existing Tokens

The application delivery web UI MUST consume the existing
`web-ui-styling` design tokens (table, form, btn, op-empty-state,
op-error-state, status). No new CSS classes are introduced for
the new section.

#### Scenario: Application list renders with the standard table

- **WHEN** the operator opens the application delivery landing
  page
- **THEN** the application list renders with the `table` token
  and any empty / error state renders with the
  `op-empty-state` / `op-error-state` tokens.

### Requirement: Application Delivery Routes Are Role-Gated

Every application delivery web route MUST be gated by the
existing role / capability registry. Owner / Admin routes
expose the lifecycle (build, promote, rollback); non-operator
users are redirected to the unauthenticated landing or to a
typed 403.

#### Scenario: Member hits a promote route

- **WHEN** a member submits `POST /applications/{id}/promote`
- **THEN** the handler returns a typed 403 and the audit event
  records the refusal without the request body.
