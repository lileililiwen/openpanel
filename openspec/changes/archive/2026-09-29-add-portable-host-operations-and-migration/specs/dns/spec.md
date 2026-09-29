# dns Specification

## Requirements

## ADDED Requirements

### Requirement: Imported Zones Reuse DNS Validation Before Cutover

DNS zones imported by a migration MUST reuse the existing zone-template and
`DnsValidation` surfaces. The imported records MUST be staged without changing
authoritative answers, and DNS cutover MUST be a separate confirmed action that
runs its own validation preflight and remains reversible for its declared
window.

#### Scenario: Imported zone validates before cutover

- **WHEN** a migration imports a domain's DNS zone
- **THEN** the records land through the existing zone surfaces without changing
  authoritative answers, and a DNS cutover is refused until its validation
  preflight passes.
