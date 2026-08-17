# dns Specification

## Purpose

The DNS bounded context covers provider accounts, zones, records,
and lifecycle. After this refinement, it also owns the
`ZoneTemplate`, `TemplateRecord`, and `TemplateName` value
objects that the template applier consumes at zone enable time.

## Requirements

### Requirement: Zone Template

The DNS bounded context SHALL model a `ZoneTemplate` carrying `name: TemplateName` (`Strict | Relaxed | Parked`) and `records: Vec<TemplateRecord>`. The constructor rejects empty template record lists.

#### Scenario: Empty template rejected

- **WHEN** `ZoneTemplate::new(TemplateName::Parked, vec![])` is called
- **THEN** the constructor returns `TemplateError::EmptyTemplate`.

### Requirement: Template Record

The DNS bounded context SHALL model a `TemplateRecord { kind, name, ttl, value, policy }`. The constructor rejects empty kind or name.

#### Scenario: Empty kind rejected

- **WHEN** `TemplateRecord::new("", "@", 60, "v=spf1", TemplateRecordPolicy::Required)` is called
- **THEN** the constructor returns `TemplateError::InvalidRecord`.

### Requirement: Built-in Templates

The DNS bounded context SHALL expose `ZoneTemplate::strict(domain)`, `ZoneTemplate::relaxed(domain)`, and `ZoneTemplate::parked()` as the three built-in templates. The strict template includes apex A/AAAA, MX, SPF, DMARC, and an optional CAA record. The relaxed template includes apex A and SPF. The parked template includes a single parking-page A record.

#### Scenario: Strict template has required records

- **WHEN** `ZoneTemplate::strict("example.com")` is built
- **THEN** `required_records().count() >= 3` and at least one SPF + one DMARC record is present.

#### Scenario: Lookup returns built-in templates

- **WHEN** `ZoneTemplate::lookup(TemplateName::Strict, ...)` is called
- **THEN** the returned template has `name = TemplateName::Strict`.

### Requirement: Behaviour Parity

The refinement introduces the new template types without changing the existing zone lifecycle. The follow-on `apply-template` change wires the applier into `DnsService::enable`.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `DnsTemplateApplied` and `DnsTemplatePreviewed` audit events. The bounded context as archived today owns the typed model and the in-memory template renderers.
