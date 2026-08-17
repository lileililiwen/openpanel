# Add plugin extension framework — Design

## Manifest

```jsonc
{
  "id": "com.example.fail2ban-ui",
  "version": "1.0.0",
  "runtime": "jsonrpc" | "wasm",
  "binary": "/opt/openpanel/plugins/fail2ban-ui/plugin-binary"
            // or "wasm_file": "fail2ban-ui.wasm"
  "entrypoint": "register",                 // JSON-RPC method
  "capabilities": [
    "system-services:read",
    "system-services:write",
    "audit:read"
  ],
  "permissions": [
    "execute_units:fail2ban",
    "read:file:/var/log/fail2ban.log"
  ],
  "ui": { "menu": "Security", "label": "Fail2ban" },
  "signature": "<ed25519 signature over the manifest body>",
  "publisher_key_id": "publisher-2025"
}
```

A panel-installed CA verifies manifest signatures; unsigned
manifests are refused with a typed error.

## Runtime

```rust
pub enum PluginRuntime {
    JsonRpc { socket: PathBuf, argv: Vec<String> },
    Wasm    { path: PathBuf, fuel: u64 },
}

pub trait PluginSupervisor {
    fn install(&mut self, manifest: PluginManifest) -> Result<PluginId, PluginError>;
    fn enable (&mut self, id: PluginId) -> Result<(), PluginError>;
    fn disable(&mut self, id: PluginId) -> Result<(), PluginError>;
    fn list   (&self) -> Vec<PluginSummary>;
    fn invoke (&self, id: PluginId, method: &str, args: Value) -> Result<Value, PluginError>;
}
```

## Capabilities

```rust
pub enum Capability {
    SitesRead, SitesWrite,
    DatabasesRead, DatabasesWrite,
    MailRead, MailWrite,
    FilesRead, FilesWrite,
    CronRead, CronWrite,
    SystemServicesRead, SystemServicesWrite,
    AuditRead,
    BackupRead, BackupWrite,
    NotificationsRead, NotificationsWrite,
    // … capability tokens are stable, versioned, and one-way
    // (no plugin can request more than its manifest declared)
}
```

A plugin can only invoke methods covered by its declared
capabilities; the JSON-RPC gateway enforces this at the host.

## Process model

Each JSON-RPC plugin runs as a child process under its own
unix user. On restart, plugins start in disabled state; the
operator explicitly enables them. WASM plugins are loaded by
`wasmtime` with a fuel budget that the supervisor refills when
the plugin handles a call.

## Endpoints

```
GET    /api/v1/plugins
POST   /api/v1/plugins/install          body: PluginManifest (or signed JSON)
POST   /api/v1/plugins/{id}/enable
POST   /api/v1/plugins/{id}/disable
DELETE /api/v1/plugins/{id}
POST   /api/v1/plugins/{id}/invoke      body: { method, args }   // Owner or Admin only
```

## CLI

```
openpanel plugin install  <manifest.json|@path>
openpanel plugin enable  <id>
openpanel plugin disable <id>
openpanel plugin list
openpanel plugin invoke  <id> <method> [args]
```

## Tests

```
1.1  Unit: manifest signature verifies; rejected manifests
      raise typed errors.
1.2  Property: capability enforcement is monotonic (a plugin
      cannot escalate at runtime); fuel is consumed exactly as
      expected.
1.3  Service tests: JSON-RPC host round-trips a fixture
      plugin; WASM load via a minimal fixture.
1.4  Integration: install a fixture plugin, enable, invoke,
      disable, uninstall.
1.5  CLI E2E: install + enable + invoke + uninstall.
1.6  Web: /plugins list with enable/disable (CSRF), invoke
      wizard.
```
