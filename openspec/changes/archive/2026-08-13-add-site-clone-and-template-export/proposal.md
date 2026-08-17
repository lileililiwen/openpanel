# Add site clone and template export

## Why

`refine-sites-with-multi-php-clone-fields` introduced
`Site::clone` (placeholder) and `Site.instance_origin_id`. This
change completes that placeholder, lets operators export a
site as a reusable **template**, and lets them clone from any
of three sources: live site, snapshot, or template. cPanel's
"Clone Website" and Baota's site export feature are the
canonical references.

## What Changes

- New bounded context `site-clone-template` consuming the
  `Site::clone` placeholder and adding the template domain.
- New domain objects: `SiteTemplate`, `TemplateArtifact`,
  `ClonePlan`, `CloneRun`.
- New endpoints:
  `POST /sites/{id}/clone`,
  `POST /sites/{id}/export-template`,
  `POST /sites/templates/{template_id}/clone-to`,
  `GET /sites/templates`.
- PII handling: anonymisation policy for user tables when
  cloning from a live site; explicit `keep_pii` flag.

## Capabilities

### New Capabilities

- `site-clone-template`: live / snapshot / template clones and
  template export.

## Impact

- Domain: `SiteTemplate`, `TemplateArtifact`, `ClonePlan`,
  `CloneRun`, `PiiPolicy`.
- App: `SiteCloneService`, `TemplateExporter`,
  `TemplateImporter` (the importer half feeds
  `add-migration-importers` style flows).
- API/CLI/web: `/sites/{id}/clone`, `/sites/{id}/export-template`,
  `/sites/templates/*`; CLI
  `openpanel site {clone,export-template,template-clone,template-list}`; web wizard.
- Coupling: depends on `sites` for chroot, `databases` for the
  staging DB, and `backups` for the artefact format.
