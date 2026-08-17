## Purpose

Introduces a stable, capability-gated plugin protocol so
third parties (and the panel's own internal modules) can ship
bounded contexts without forking the codebase. Manifests are
signed; capabilities are pre-declared and enforced at every
syscall; JSON-RPC and Wasm runtimes are supported behind a
single supervisor.

# plugin-extension-framework Specification

## Requirements

### Requirement: Signed Manifest

Every plugin SHALL ship a `PluginManifest` describing
`id`, `version`, `runtime` (`JsonRpc | Wasm`), entrypoint,
declared capabilities, declared permissions, optional UI
metadata, and an Ed25519 signature under a panel-installed
publisher key. Unsigned or invalid signatures MUST be refused
with `PluginError::InvalidManifestSignature`; the last
known-good plugin registry is preserved on signature failure.

#### Scenario: Valid manifest installs

- **WHEN** an Owner submits a manifest whose signature verifies
- **THEN** a `PluginInstall` row is created with status `Installed` and the audit `PluginInstalled` is recorded.

#### Scenario: Tampered manifest rejected

- **WHEN** one byte of the manifest body is altered before signature verification
- **THEN** the install is refused; audit `PluginManifestRejected` records the id and redacted reason; the registry is unchanged.

### Requirement: Capability Gating

A plugin SHALL only invoke methods covered by its declared
capabilities. The supervisor MUST enforce this at every host
syscall, including JSON-RPC, WASM, and any retry path.
Capabilities are stable, versioned tokens; a plugin MUST NOT
be able to escalate at runtime even if a host bug is exploited.

#### Scenario: In-scope call

- **WHEN** a plugin with `system-services:read` invokes `service.list()`
- **THEN** the host returns the typed payload.

#### Scenario: Out-of-scope refused

- **WHEN** the same plugin invokes `service.restart("fail2ban")`
- **THEN** the host returns `CapabilityDenied{required=system-services:write}` and audit `PluginCapabilityDenied` records the required capability only.

### Requirement: Lifecycle and Process Isolation

A plugin SHALL run as a child process (JSON-RPC) or a sandboxed
WASM instance under a unique unix user. Plugins MUST start
disabled and require explicit enablement. The supervisor MUST
restart a crashed JSON-RPC plugin at most 3 times within a
60-second window before marking it `Failed`.

#### Scenario: Restart on crash

- **WHEN** a JSON-RPC plugin crashes
- **THEN** the supervisor restarts it and emits `PluginRestarted{attempt=N}`.

#### Scenario: Crash budget

- **WHEN** a plugin has crashed 3 times in 60s
- **THEN** the plugin is marked `Failed` and the audit `PluginDisabledDueToCrashBudget` is recorded.

#### Scenario: Disable keeps state

- **WHEN** an Owner disables a plugin
- **THEN** its data persists; subsequent invocations return `PluginDisabled`.

### Requirement: UI Surface

A plugin MAY expose UI metadata (`menu`, `label`, `icon`).
The web shell SHALL render a menu entry whose handler is a
typed JSON-RPC call to the plugin; web mutations enforce CSRF
and re-validate capabilities per call.

#### Scenario: Menu entry renders

- **WHEN** a plugin declares `ui.menu=Security`
- **THEN** the shell renders a sidebar entry; clicking it
        opens a typed iframe or an HTMX-rendered page that
        issues JSON-RPC calls.

#### Scenario: Out-of-scope UI invocation refused

- **WHEN** the UI attempts a capability the plugin does not have
- **THEN** the JSON-RPC host denies and emits `PluginCapabilityDenied`.

### Requirement: Audit and Observability

Every plugin invocation SHALL be audit-logged with
`plugin_id`, `method`, redacted arg-keys, latency, and
outcome. Audit MUST NOT include request or response bodies
that may carry secrets.

#### Scenario: Invocation logged

- **WHEN** a plugin invokes a method
- **THEN** audit `PluginInvocation{plugin_id, method, arg_keys, latency_ms, outcome}` is recorded with no payload data.

#### Scenario: Secret in arg refused

- **WHEN** a plugin invokes with an arg key the panel marks sensitive (e.g. `password`)
- **THEN** the invocation is refused pre-empt with `PluginSecretArgRejected`; no audit body is written.
