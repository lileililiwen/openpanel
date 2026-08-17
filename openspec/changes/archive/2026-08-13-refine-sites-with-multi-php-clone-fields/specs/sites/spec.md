## ADDED Requirements

### Requirement: PhpRuntimeRef Site Field

The site aggregate SHALL record an optional `php_runtime: Option<PhpRuntimeRef>` describing the PHP-FPM runtime that will serve the site. In v0.1 the value MUST be `None` and the field is metadata only; in v0.2 the value MUST reference a row in `software_center.installed_components`. A site with `php_enabled=true` and `php_runtime=None` MUST be rejected with `SitesError::PhpRuntimeMissing` once multi-PHP support is announced; until then the field is allowed to be null.

#### Scenario: v0.1 site stores null runtime

- **WHEN** a v0.1 site is created with `php_enabled=false`
- **THEN** the persisted row has `php_runtime IS NULL` and `Site::php_runtime() == None`.

#### Scenario: v0.2 rejects missing runtime

- **WHEN** v0.2 is announced and a site is updated with `php_enabled=true, php_runtime=None`
- **THEN** the update fails with `SitesError::PhpRuntimeMissing` and no DB write occurs.

### Requirement: Clone Origin and Template Site Fields

The site aggregate SHALL record two optional identity references: `instance_origin_id` (the source site this site was cloned from) and `clone_template_id` (the template used to seed this site, set when the clone is derived from a template rather than a live site). Both fields are nullable. The repository SHALL provide lookup helpers `find_by_clone_template_id` and `find_by_instance_origin_id`.

#### Scenario: Round-trip preserves fields

- **WHEN** a Site row with all three new fields non-null is read back
- **THEN** all three fields round-trip exactly with no precision loss.

#### Scenario: Lookup by clone template

- **WHEN** `find_by_clone_template_id(<id>)` is called
- **THEN** every site derived from that template is returned in chronological order.

#### Scenario: Lookup by origin

- **WHEN** `find_by_instance_origin_id(<id>)` is called
- **THEN** every site whose `instance_origin_id == <id>` is returned.

### Requirement: Site Clone Helper (Placeholder)

The site aggregate SHALL expose a `Site::clone(source, target_domain, new_owner_id)` helper returning a `DraftSite` placeholder. The placeholder is not persisted by this change; the follow-on `add-site-clone-and-template-export` change implements persistence. The helper SHALL rebase `document_root` and SHALL copy `php_runtime`, `clone_template_id` defaults, and `instance_origin_id` from the source.

#### Scenario: Clone produces a draft

- **WHEN** `Site::clone(&source, target_domain, new_owner_id)` is called
- **THEN** the result is a `DraftSite` whose `document_root` is the source's root rebased to the new domain, whose `instance_origin_id == Some(source.id)`, and whose `id` is `None`.

#### Scenario: Clone is not persisted

- **WHEN** the helper is invoked by tests or by services
- **THEN** no DB row is created by this change.
