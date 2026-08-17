# Add plugin extension framework

## Why

OpenPanel is currently a closed system: every bounded context
that ships is the result of an OpenSpec change. Baota has had
a plugin store for years; cPanel's modular structure is the
canonical example. Without an extension story the panel cannot
match partner ecosystems, and operators cannot ship in-house
recipes without forking the project. This change defines a
**signed recipe protocol** (JSON-RPC + Wasm option) that lets
third parties contribute bounded contexts that run inside the
panel's process with capability-gated access.

## What Changes

- New bounded context `plugin-extension-framework` carrying
  the manifest spec, the runtime adapter, and the capability
  model.
- New endpoints: `POST /plugins/install`,
  `POST /plugins/{id}/enable`, `POST /plugins/{id}/disable`,
  `DELETE /plugins/{id}`, `GET /plugins`.
- Capability model: typed capability tokens the panel grants
  at install time; revoke at uninstall.
- Two runtimes:
  - **JSON-RPC over localhost Unix socket** — any language
    able to speak JSON-RPC, executed as a child process.
  - **Wasm** — single store compiled with `wasmtime`, sandboxed
    by capability.
- A plugin CLI built around the same protocol so OpenPanel's
  own internal bounded contexts can sit in the same registry
  as third-party ones.

## Capabilities

### New Capabilities

- `plugin-extension-framework`: signed manifests, capability
  gating, JSON-RPC and Wasm runtimes.

## Impact

- Domain: `PluginManifest`, `Capability`, `CapabilityGrant`,
  `PluginRuntime`.
- App: `PluginRegistry`, `PluginSupervisor`, `JsonRpcHost`,
  `WasmRuntime`.
- API/CLI/web: `/plugins/*`; CLI `openpanel plugin`; web
  `/plugins` page.
- Security: each plugin runs under a unique unix user; the
  WASM runtime enforces capability tokens at every syscall.
