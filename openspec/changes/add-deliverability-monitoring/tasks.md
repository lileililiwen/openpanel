# Add Email deliverability monitoring — Tasks

## 1. Testing

- [ ] 1.1 Unit: DMARC aggregate parser — fixture XML with two rows
      yields correct per-source stats; input >10 MiB rejected;
      DOCTYPE/entity-bearing XML rejected before parse; malformed XML
      returns `DeliverabilityError::ReportMalformed` (no panic).
- [x] 1.2 Unit: DNSBL query-name construction — IPv4 `192.0.2.5`
      against zone `zen.spamhaus.org` →
      `5.2.0.192.zen.spamhaus.org`; IPv6 nibble expansion verified.
      (Listing upsert semantics also landed with this slice.)
- [ ] 1.3 Unit: listing upsert preserves `first_seen` on repeat
      listings and sets `resolved_at` when the check clears.
- [ ] 1.4 Unit: auth-audit drift — SPF record missing `all`
      mechanism, DKIM selector key mismatch, and DMARC rua absent each
      produce exactly one drift entry with a stable code.
- [ ] 1.5 Property: for arbitrary well-formed report rows (≥100
      cases) parsed stats sum to the row count per source-day and no
      field retains raw message identifiers beyond IP/day counters.
- [ ] 1.6 Integration (`tests/integration/deliverability.rs`) with
      `MockResolver`: listed IP triggers notification event through
      mock dispatcher; clear resolves; on-demand check endpoint
      returns summary; unauthenticated → 401.
- [ ] 1.7 Integration: 90-day prune removes older `DmarcSourceStat`
      rows after ingest.
- [ ] 1.8 CLI E2E: `cli_mail_deliverability_check_then_show`.
- [ ] 1.9 Web: Deliverability tab at 360/768/1280 px; screenshots.

## 2. Domain

- [ ] 2.1 Add types + pure parser + validation under
      `crates/openpanel-domain/src/deliverability/`; define
      `ResolverPort`.

## 3. Application

- [ ] 3.1 Resolver impl (app layer), scheduler as background task,
      repo + migrations, retention prune.
- [ ] 3.2 Service wiring check→notify via notifications port.

## 4. Adapters and UI

- [ ] 4.1 REST routes per design.
- [ ] 4.2 CLI subcommands.
- [ ] 4.3 Web tab.

## 5. Validation

- [ ] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean.
- [ ] 5.3 Smoke-test: point blocklist at an zone returning 127.0.0.2
      for a test IP (local resolver stub), observe listing + alert;
      restore, observe resolution.
- [ ] 5.4 Archive with `openspec archive add-deliverability-monitoring`.
