## Purpose

Completes the `Site::clone` placeholder introduced by the
sites refinement, adds **template export** so a site can be
reused across the panel, and lets operators clone from a
live site, a snapshot, or a template with explicit PII
handling.

# site-clone-template Specification

## Requirements

### Requirement: Clone Source Discrimination

The system SHALL accept a `CloneSource` of
`Site | Snapshot | Template` and produce a `ClonePlan` for
each. The plan MUST list every file copied, every DB action,
the anonymisation policy, the warnings (overwrites, missing
extensions, ignored globs), and a `content_hash` for
verification at run time.

#### Scenario: Source = live site

- **WHEN** an Owner clones from a live site
- **THEN** the plan's files come from the site's document
        root; the DB action is `dump_and_load`; PII anonymisation
        is "standard" unless `keep_pii=true`.

#### Scenario: Source = template

- **WHEN** an Owner clones from a template id
- **THEN** the plan's files come from the template artifact;
        the DB action is `import_schema_with_placeholder_data`;
        PII is never present in templates.

### Requirement: Confirmed Run

`POST /sites/{id}/clone` SHALL accept `confirmed_at` within
±60 seconds and a `plan_id`. The run MUST honour the plan's
chroot boundaries (the source must be a site the caller can
read) and MUST fail if the `content_hash` of the source has
changed since the plan.

#### Scenario: Source unchanged

- **WHEN** the run starts and the source content_hash matches
- **THEN** the run proceeds to file copy, DB load, anonymisation,
        and audit `SiteCloned{from, to, policy}`.

#### Scenario: Source changed

- **WHEN** the source has been modified since plan
- **THEN** the run is refused with `SourceContentChanged` and
        no state changes.

### Requirement: PII Handling

Templates SHALL NEVER include plaintext PII. A live clone
default-anonymises by replacing `users.email` with
`<uuid>@template.local` and rotating `users.password_hash`;
the original token mapping is encrypted under the master
key in `anonymisation_tokens` and reverseable only with the
mapping KEK.

#### Scenario: keep_pii is false (default)

- **WHEN** an Owner clones a site with no flag
- **THEN** all `users.email` rows are replaced; no plaintext
        PII is present in the clone; audit `ClonePiiAnonymised`
        records the row count.

#### Scenario: keep_pii is true

- **WHEN** the Owner explicitly sets `keep_pii=true`
- **THEN** the panel refuses the clone unless `confirmed_at`
        is fresh and the caller's role is Owner; audit
        `CloneKeptPii` records the source and target ids.

### Requirement: Template Export Lifecycle

`POST /sites/{id}/export-template` SHALL tar the site's
content with a deny-list (`node_modules`, `.git`,
`storage/logs/*`, `*.lock`), produce a `template.json` with
site metadata, and sign the manifest with the panel's
Ed25519 publisher key. The template artifact is stored under
`template_artifacts/<id>.tar.gz.<ext>` with mode 0600.

#### Scenario: Valid export

- **WHEN** an Owner exports a templatable site
- **THEN** the artifact path is returned, the signature
        verifies, and a `SiteTemplate` row is persisted.

#### Scenario: Deny-list violation

- **WHEN** the export's tar capture encounters a forbidden path
- **THEN** the path is skipped with a warning; the artifact is
        still produced; the warning is recorded on
        `SiteTemplate.warnings`.

#### Scenario: Signature failure on retrieval

- **WHEN** the artifact is later retrieved and its signature
        does not verify
- **THEN** the template is refused with `TemplateSignatureFailed`
        and the audit logs the template id only.

### Requirement: Template Re-Application

`POST /sites/templates/{tid}/clone-to` SHALL produce a new
site under the caller-specified `target_domain` and
`target_owner_id`. The clone SHALL honour the same
anonymisation, chroot, and run-confirmation rules.

#### Scenario: Re-apply with new domain

- **WHEN** an Owner clones a template to `staging.example.com`
- **THEN** a new `Site` is persisted with
        `instance_origin_id = Some(<template source site>)`
        and the audit `SiteClonedFromTemplate{template, target}` is recorded.
