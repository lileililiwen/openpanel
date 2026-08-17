# Add plugin extension framework — Tasks

## 1. Testing

- [ ] 1.1 Unit tests: manifest signature; rejected manifests.
- [ ] 1.2 Property tests: capability monotonicity (1000 cases);
      fuel invariant.
- [ ] 1.3 Service tests: JSON-RPC fixture; WASM fixture.
- [ ] 1.4 Integration: install / enable / invoke / disable /
      uninstall on a fixture plugin.
- [ ] 1.5 CLI E2E: full lifecycle.
- [ ] 1.6 Web: `/plugins` with CSRF.

## 2. Domain and Application

- [ ] 2.1 Add `PluginManifest`, `Capability`, `CapabilityGrant`,
      `PluginRuntime` under
      `crates/openpanel-domain/src/plugin_extension_framework/`.
- [ ] 2.2 Implement `PluginRegistry`, `JsonRpcHost`,
      `WasmRuntime` (via `wasmtime`).
- [ ] 2.3 Add SQLite migration for `plugin_installs`,
      `plugin_capabilities`, `plugin_runs`.

## 3. Adapters and UI

- [ ] 3.1 Add `/plugins/*` REST routes.
- [ ] 3.2 Add `openpanel plugin {install,enable,disable,list,invoke}`.
- [ ] 3.3 Build `/plugins` page (CSRF).

## 4. Validation

- [ ] 4.1 `cargo test --workspace` twice.
- [ ] 4.2 `make check` clean.
- [ ] 4.3 Smoke-test: install a fixture plugin that surfaces
      under the menu; verify capability gating refuses
      out-of-scope calls.
- [ ] 4.4 Archive with `openspec archive add-plugin-extension-framework`.
