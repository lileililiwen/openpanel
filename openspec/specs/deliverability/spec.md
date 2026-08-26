# deliverability Specification

## Purpose
TBD - created by archiving change add-deliverability-monitoring. Update Purpose after archive.
## Requirements
### Requirement: DNSBL Listing Checks

The system SHALL query configured DNSBL zones for the host's outbound
IPs and each mail domain's MX/A records on a schedule and on demand,
recording listings with preserved first-seen timestamps and resolved
timestamps when cleared.

#### Scenario: New listing alerts

- **WHEN** a scheduled check finds an IP newly listed in a configured
        zone
- **THEN** a `Listing` row is stored and a notification is dispatched
        through the user's subscribed channels.

#### Scenario: Resolution recorded

- **WHEN** a previously listed IP no longer answers in that zone
- **THEN** the listing's `resolved_at` is set and it no longer counts
        against the domain's status.

### Requirement: Authentication Alignment Audit

The system SHALL validate each mail domain's published SPF, DKIM, and
DMARC records against the panel-managed values and report drift as a
structured audit result.

#### Scenario: Drift detected

- **WHEN** a domain's live SPF text differs from the managed policy
- **THEN** the audit lists a stable drift code for SPF while leaving
        other checks' results intact.

### Requirement: DMARC Aggregate Report Ingestion

The system SHALL ingest DMARC aggregate reports addressed to a
domain's RUA mailbox, parsing them into per-source daily statistics
(messages, DKIM pass, SPF pass) retained for 90 days. Reports larger
than 10 MiB or containing entities SHALL be rejected before parsing.

#### Scenario: Stats accumulated

- **WHEN** a valid report covering two sources arrives
- **THEN** per-source day statistics are upserted and retrievable via
        the sources endpoint.

#### Scenario: Oversize rejected

- **WHEN** a report exceeds the size cap
- **THEN** it is discarded with `ReportMalformed`-class handling and
        nothing is stored.

### Requirement: Deliverability Surfaces

Owners SHALL view deliverability status, run on-demand checks, list
source statistics, and configure the checked blocklist zones via API,
CLI, and web; configuration changes SHALL be audited.

#### Scenario: Summary round-trip

- **WHEN** an Owner opens the Deliverability tab after a clean check
- **THEN** the domain shows no listings, current auth-audit state, and
          the last check timestamp.

