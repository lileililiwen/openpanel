# sites Specification

## Requirements

## ADDED Requirements

### Requirement: Imported Sites Reuse the Existing Site Workspace

A site imported by a migration MUST reuse the existing site workspace, site
staging, and audit surfaces rather than creating a parallel site model. Only
resources explicitly included in the confirmed plan are committed, and the
imported site MUST be visible in the same per-site workspace an operator uses
for a natively created site.

#### Scenario: Imported site reuses existing site workspace

- **WHEN** a migration commits a site that does not conflict with an existing
  domain
- **THEN** the imported site appears in the existing site workspace with the
  same staging and audit behaviour as a natively created site.
