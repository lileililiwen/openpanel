# Config pages for managed software — Tasks

## 1. Testing

- [x] Extend `crates/openpanel-app/tests/software_center.rs` with the
      read/save/gate tests in the design.
- [x] Add web integration coverage for the configuration page, save,
      CSRF, and role gates in `tests/integration/software_center.rs`.
- [x] Add JSON API integration coverage for configuration reads, saves,
      and role gates in `tests/integration/software_center.rs`.

## 2. Server: config manifest and service

- [x] Add `crates/openpanel-app/src/software_center/config.rs` with the
      `ComponentConfig` struct and `component_config(&str)` table.
- [x] Add `config` module to the software center and `config_root` field
      to the service (default `/`, sandbox param in
      `with_artifact_pipeline` and `memory_with_artifact_and_gate`).
- [x] Implement `component_config_info`, `read_config`, `save_config`
      with owner gating and the atomic write + restore path.
- [x] Wire validation via `self.packages.validate(component)`.

## 3. Web: pages and routes

- [x] Detail page: `Configuration` action for managed components with a
      manifest entry.
- [x] Add `GET/POST /software/components/{id}/config` handlers and
      routes with CSRF, banners, and the restored-content error state.

## 4. API: JSON endpoints

- [x] Add `GET/POST /api/v1/software/components/{id}/config` and wire
      them in `crates/openpanel-api/src/routes/software_center.rs`.

## 5. Verification

- [x] `cargo test -p openpanel-app -p openpanel-web -p openpanel-api`.
- [x] Run the config-focused HTTP integration tests.
- [x] Run `make check` and `openspec validate
      2026-08-13-config-pages-for-managed-software`.
