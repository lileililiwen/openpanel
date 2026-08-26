# ui-state-vocabulary Specification

## Purpose
TBD - created by archiving change add-admin-interaction-surface. Update Purpose after archive.
## Requirements
### Requirement: Reusable UI State Components

The web adapter SHALL provide four reusable maud components in
`crates/openpanel-web/src/ui_states.rs`:

- `EmptyState { title, body, cta_href, cta_label }`
- `NoResultsState { query }`
- `LoadingState { label }`
- `ErrorState { title, body, retry_href }`

Each component SHALL render with stable CSS classes (`op-empty-state`,
`op-no-results`, `op-loading-state`, `op-error-state`), SHALL be
keyboard-accessible, and SHALL use ARIA live regions (`aria-live="polite"`
or `aria-live="assertive"` for the error state) so assistive technology
announces transitions.

#### Scenario: All four components share stable classes and ARIA

- **WHEN** any route renders `EmptyState`, `NoResultsState`, `LoadingState`,
  or `ErrorState`
- **THEN** the rendered HTML carries the expected stable CSS class
  (`op-empty-state` / `op-no-results` / `op-loading-state` /
  `op-error-state`) and an `aria-live` attribute (`polite` for empty /
  no-results / loading, `assertive` for error)

### Requirement: List Routes Render an Empty State

Every list route in the web adapter (sites, databases, cron, backups,
SSL, files, users, DNS, FTP, mail, API tokens, audit, monitoring, logs,
notifications) SHALL render one of `EmptyState`, `NoResultsState`, or
its data — NEVER an empty `<table>` body and NEVER a blank panel.

#### Scenario: Empty sites list shows CTA

- **WHEN** the authenticated user opens `/sites` and no sites exist
- **THEN** the page renders `<EmptyState title="No sites yet" body="..."
  cta_href="/sites/new" cta_label="Create your first site" />` and the
  page does not render an empty table

#### Scenario: Filtered list with no matches shows NoResultsState

- **WHEN** the user filters the sites list by a query that returns zero rows
- **THEN** the page renders `<NoResultsState query="..." />` with a link
  to clear the filter

### Requirement: Long-Running Actions Render a Loading State

Every form whose handler may take more than 250ms SHALL include
`hx-indicator="#op-loading-{action}"` and the shell SHALL mount the
matching indicator element so the user sees visible feedback during the
request. The `LoadingState` component SHALL be used by the indicator.

#### Scenario: Submitting site-create form shows loading

- **WHEN** a user submits the site-create form
- **THEN** the matching `#op-loading-create-site` indicator becomes
  visible during the request and is hidden by `htmx:afterRequest`

### Requirement: Recoverable Error State

Every list route and every form route SHALL render an `ErrorState` (not
a raw error string) when its handler returns `5xx` or fails to render.
The error state SHALL include a `Retry` button (`hx-get` to the same
endpoint) and SHALL use `aria-live="assertive"`.

#### Scenario: 500 from sites list shows error with retry

- **WHEN** `GET /sites` returns `500`
- **THEN** the page renders `<ErrorState title="..." body="..." retry_href="/sites" />`
  with a working `Retry` link, and no raw stack trace is exposed to the browser

