# audit-activity Specification

## Purpose
TBD - created by archiving change add-audit-activity-center. Update Purpose after archive.
## Requirements
### Requirement: Searchable audit history

The system MUST provide an owner-only audit page and API with actor, action, target, outcome, time-range, and cursor filters.

#### Scenario: Owner filters events

- **WHEN** an owner submits valid filters
- **THEN** results are ordered newest-first and include a deterministic next cursor

### Requirement: Audit data is redacted

The system MUST exclude passwords, tokens, private keys, request bodies, and non-allowlisted metadata from every audit response.

#### Scenario: Secret appears in stored metadata

- **WHEN** an event contains secret-shaped metadata
- **THEN** the HTML and JSON representations omit or redact it

### Requirement: Audit UI states are explicit

The audit page MUST render distinct loading, empty, no-results, error, and success states.

#### Scenario: No event matches filter

- **WHEN** a valid filter returns no events
- **THEN** the page shows a no-results state with a clear-filters action

