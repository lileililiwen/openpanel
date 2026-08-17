# sites Specification

## Purpose

The sites bounded context covers site lifecycle, aliases, document
root, and PHP enablement. After this refinement, it also owns
the optional `php_runtime`, `clone_template_id`, and
`instance_origin_id` fields that the per-site-php-runtime and
clone / template-export changes consume.

## Requirements

### Requirement: Per-Site PHP Runtime Reference

The `Site` aggregate SHALL carry an optional `php_runtime: Option<PhpRuntimeRef>` field. When `None`, the site is not associated with a PHP runtime; the per-site-php-runtime bounded context fills the field by calling `Site::set_php_runtime`.

#### Scenario: New site has no runtime

- **WHEN** a new site is created via `Site::new`
- **THEN** `php_runtime` is `None`.

### Requirement: Clone Template and Instance Origin

The `Site` aggregate SHALL carry `clone_template_id: Option<Uuid>` and `instance_origin_id: Option<Uuid>`. Both default to `None` on a fresh site. The `Site::clone` helper builds a draft site with `instance_origin_id = source.id`.

#### Scenario: Clone records origin

- **WHEN** `Site::clone(source, new_id, target_domain, new_owner_id, actor)` is called
- **THEN** the returned site has `instance_origin_id = source.id` and a fresh `id`.

### Requirement: Behaviour Parity

The refinement introduces the new fields without changing the existing lifecycle (`enable`, `disable`, `change_*`). The follow-on `add-per-site-php-runtime` and `add-site-clone-and-template-export` changes populate the fields and own the install / swap / file copy paths.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `SitePhpRuntimeAssigned`, `SiteCloned`, and `SiteTemplateExported` audit events. The bounded context as archived today owns the typed model and the pure-function `Site::clone` helper.
