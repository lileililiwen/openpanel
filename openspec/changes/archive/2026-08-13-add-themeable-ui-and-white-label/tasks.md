# Add themeable UI and white-label — Tasks

## 1. Testing

- [x] 1.1 Unit tests: contrast ratio computation; logo mime
      validation; fqdn normalisation.
- [x] 1.2 Property tests: contrast_min enforced; theme override
      never modifies master tokens.css (1000 cases).
- [x] 1.3 Service tests: PUT/GET/clear; palette validation;
      logo upload + serving.
- [x] 1.4 Integration: override applied per host header; default
      for unknown host.
- [x] 1.5 CLI E2E: set / palette / clear.
- [x] 1.6 Web: `/admin/branding` editor (CSRF); live preview.

## 2. Domain and Application

- [x] 2.1 Add `ThemeOverride`, `Palette`, `Typography`,
      `PanelDomain` under
      `crates/openpanel-domain/src/themeable_ui/`.
- [x] 2.2 Implement `BrandingService` and register the module.
- [x] 2.3 Implement contrast enforcement.

## 3. Adapters and UI

- [x] 3.1 Add `/admin/branding/*` REST routes.
- [x] 3.2 Add `openpanel branding` CLI subcommands.
- [x] 3.3 Build `/admin/branding` editor (CSRF).

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [ ] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: upload a logo + change palette; contrast
      warning when colour violates WCAG AA.
- [x] 4.4 Archive with `openspec archive 2026-08-13-add-themeable-ui-and-white-label`.
