# Progress — repair-ui-discoverability

## Research
- Confirmed the original defect: `CapabilitySet::shipped()` enumerated only 10 of 24
  first-class capabilities, so mail/dns/cron/backups/logs/security/software/docker/registry/
  webmail/plugins/marketplace/container-registry/themeable-ui nav items were hidden.
- The Status-page nav item referenced an unregistered `pulse` icon, failing
  `nav_model::tests::every_nav_item_has_a_builtin_icon`.

## Implementation
- `nav_model.rs`: added the `pulse` inline-SVG icon path.
- `layout.rs`: expanded `CapabilitySet::shipped()` to the canonical 24-capability inventory
  and added public `contains`/`iter` accessors.

## Tests added
- `nav_model::tests::every_nav_item_capability_is_registered` — nav caps ⊆ shipped().
- `nav_model::tests::every_shipped_capability_has_a_nav_item` — shipped() ⊆ nav caps.
- `layout::tests::owner_sees_all_shipped_workflows_in_navigation`.
- `layout::tests::user_sees_only_authenticated_workflows`.
- Updated two stale layout tests that encoded the old hidden behavior.

## Verification
- `cargo test -p openpanel-web --lib`: 134 passed (was failing on the icon test).
- `openspec validate repair-ui-discoverability --strict`: valid.
- `make check`: blocked by pre-existing, unrelated failures (openpanel-domain `status_page.rs`).
