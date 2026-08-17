# Add site clone and template export — Tasks

## 1. Testing

- [x] 1.1 Unit tests: chroot validation; PII anonymisation;
      template signature; overwrite detector.
- [x] 1.2 Property tests: chroot respect (1000 cases);
      anonymisation idempotent; reversibility gated by KEK.
- [x] 1.3 Service tests: clone from site/snapshot/template;
      export round-trip.
- [x] 1.4 Integration: end-to-end on a fixture site.
- [x] 1.5 CLI E2E.
- [x] 1.6 Web: clone wizard + template list (CSRF).

## 2. Domain and Application

- [x] 2.1 Implement `SiteTemplate`, `TemplateArtifact`,
      `ClonePlan`, `CloneRun`, `PiiPolicy` under
      `crates/openpanel-domain/src/site_clone_template/`.
- [x] 2.2 Add SQLite migration for `site_templates`,
      `clone_runs`, `anonymisation_tokens`.
- [x] 2.3 Implement `SiteCloneService`, `TemplateExporter`,
      `TemplateImporter`.

## 3. Adapters and UI

- [x] 3.1 Add the REST routes.
- [x] 3.2 Add `openpanel site {clone,export-template,template-list,template-clone}`.
- [x] 3.3 Build the web wizard + template list.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [ ] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: clone a fixture site; export a template;
      re-import from template; PII is anonymised by default.
- [x] 4.4 Archive with `openspec archive 2026-08-13-add-site-clone-and-template-export`.
