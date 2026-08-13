# Config pages for managed software — Design

## Overview

Delivery target: a business owner clicks **Configuration** on an
installed component's detail page, edits its key config file in a
textarea, and saves. The panel does the file I/O; the user never sees a
path or a terminal.

```text
entry detail (software center)        config page        save
      |  (managed + manifest)                |              |
      |-- Configuration link --------------->|              |
                                              GET read_config
                                              -> textarea     |
                                              POST save_config|--> atomic write
                                                                --> validate (if any)
                                                                --> restore on failure
                                                                --> banner
```

## Boundary and security model

* The config manifest is **server code** (a static table), not data from
  the signed catalog. A compromised or hostile remote catalog cannot
  add arbitrary writable paths.
* Reads and writes are **Owner-only**, mirroring every other Software
  Center mutation (`owner(role)?`).
* Saves require the component to be **panel-managed**
  (`managed_components()`), so an operator cannot edit config for
  software the panel does not own.
* Only the manifest's key file is editable in this phase. Editing
  arbitrary files stays the job of the Files module.
* File size is bounded on read (128 KiB) and write (the same ceiling),
  preventing a huge config from pinning memory or the form.

## Component structure

New module `crates/openpanel-app/src/software_center/config.rs`:

```rust
pub struct ComponentConfig {
    pub component: &'static str,          // catalog id, e.g. "nginx"
    pub label: &'static str,              // "nginx.conf"
    pub files: &'static [&'static str],   // rel. to service config_root, key file first
    pub validatable: bool,                // may run the package validate() pass
}

pub fn component_config(component: &str) -> Option<&'static ComponentConfig>
```

Table entries: fail2ban, nginx, php-8.0..php-8.3, mysql, mariadb, redis
(paths listed in the proposal). Only the key file (`files[0]`) is
editable in phase 1; extra files are shown read-only for context in the
page metadata and reserved for later phases.

## Service surface (`SoftwareCenterService`)

* `pub async fn component_config_info(&self, role, component)
  Result<ComponentConfigInfo, SoftwareCenterError>` — owner-only;
  requires a manifest entry AND a managed component. Returns `component`,
  `label`, the absolute `path`, and whether the file `exists`.
* `pub async fn read_config(&self, role, component)
  Result<ComponentConfigDocument, SoftwareCenterError>` — owner-only;
  returns `{ component, label, path, exists, content }` where content is
  bounded to 128 KiB. A missing file is NOT an error (`exists: false`,
  `content: None`).
* `pub async fn save_config(&self, role, component, content)
  Result<String, SoftwareCenterError>` — owner-only; atomic write;
  when `validatable`, calls `self.packages.validate(component)` and on
  failure restores the previous content and returns the validation
  error. Returns the absolute path saved (for the success banner).
  Content is bounded to 128 KiB on the way in.

Gate helper: `async fn require_managed_config(&self, component, role)
-> Result<&'static ComponentConfig, SoftwareCenterError>` rejects
non-owners (`Forbidden`), unknown components (`Unsupported`), and
managed-inventory misses (`Forbidden`).

## Config root

`config_root` is a new `PathBuf` field on `SoftwareCenterService`
defaulting to `/`. To keep the existing `with_artifact_pipeline`
constructor signature stable for the eight direct callers in the
app-crate test suite, the constructor chain threads `config_root`
through the module layer (`SoftwareCenterModule::compose_with_artifact`
gains a `config_root: PathBuf` parameter,
`memory_with_artifact_and_gate` gains the same parameter, and
`memory_with_artifact` / `compose` pass `default_config_root()`). The
service exposes a builder `with_config_root(self, root) -> Self` so the
app-crate tests can sandbox the root without churning every existing
test call. `TestServer` passes `sandbox/config`, threads it through
`memory_with_artifact_and_gate`, and exposes `config_root()` so tests
can stage fixture files.

## Atomic write

Mirrors `sites::nginx::write_with_test`:

1. resolve absolute path = `config_root.join(rel)` (relative paths in
   the manifest let the sandbox work without `Path` self-join
   surprises);
2. create parent dirs (`tokio::fs::create_dir_all`);
3. write `path + ".new"` (sibling so `rename` is atomic), then
   `rename` over the target;
4. if `validatable` and `packages.validate(component)` returns
   `Err`, restore the previous bytes (or remove the file if it did
   not exist) and return the error;
5. return the path.

## Web surface

* `detail_content(...)` gains a `has_config: bool` flag (computed by
  the `entry` handler from `component_config(id).is_some()`); when the
  entry is `panel_managed` and has a manifest entry, a `Configuration`
  link joins Update/Remove in the actions row, linking to
  `GET /software/components/{id}/config`.
* New routes in `crates/openpanel-web/src/router.rs`:
  - `GET /software/components/{id}/config` → `config_page` renders the
    shell page: breadcrumb, component name, absolute path in a
    read-only line, a `<textarea name="content">`, Save (csrf), Cancel,
    and a success/error banner carried over via a page-level render.
  - `POST /software/components/{id}/config` → `config_save` verifies
    csrf, calls `save_config`, re-reads the document (so the page
    always shows the authoritative on-disk content — the previous
    content after a failed validation), and re-renders with a
    success or error banner.
* Both reuse `render_shell` and the existing markup classes
  (`button`, `button--danger`, `button--ghost`). New CSS in
  `app.css`: `.config__path`, `.banner`, `.banner--ok`, `.banner--error`
  (using the existing status hex colors; no new variables).
* `config_error_message` translates the typed
  `SoftwareCenterError` variants into owner-readable text — distinct
  from the install-flow `software_center_error_message` because the
  wording for `Validation` ("previous content was restored") is
  config-specific.

## JSON API (`openpanel-api`)

* `GET /api/v1/software/components/{id}/config` →
  `ComponentConfigDocument` (the same struct the web page consumes).
* `POST /api/v1/software/components/{id}/config` with `{ content }` →
  `{ "path": "..." }` or an error mapped through the existing
  `map` function (`Forbidden` → 403, `Invalid` → 422,
  `Validation` / `Package` / `Repository` → 500, `Unsupported` → 422).
* Wired in `crates/openpanel-api/src/routes/software_center.rs`
  (the only behavior change to the existing routes file is the two
  additional route registrations).

## Error mapping

New errors are expressed with existing variants:

* not owner → `Forbidden`
* unknown component or no manifest → `Unsupported`
* component installed but not panel-managed → `Forbidden`
* file missing on read → `ComponentConfigDocument { exists: false }`
  (not an error)
* validation failure after write → `Validation` (content already
  restored)
* config file IO failure → `Package(String)` carrying the redacted
  `std::io::Error` detail

## Testing

App tests (append to `crates/openpanel-app/tests/software_center.rs`),
using `with_artifact_pipeline(...).with_config_root(sandbox)`:

* `read_config` returns the fixture file content for a managed
  component.
* `read_config` returns `exists: false` for a missing file.
* `read_config` rejects a file over the 128 KiB ceiling.
* `save_config` writes bytes readable back verbatim.
* `save_config` validation failure (driven by an `EventuallyFailingValidate`
  package manager that fails on the second validate call) restores
  the previous content.
* `read_config` / `save_config` / `component_config_info` reject
  non-owner roles.
* Unknown component id is rejected with `Unsupported`.
* Redis component not in `managed_components` is rejected with
  `Forbidden`.

Integration tests (append to
`tests/integration/software_center.rs`):

* Web: `config_page_renders_and_saves_a_managed_component` — install
  redis via the API, write a fixture file under `config_root`, log in
  via the web form, verify the detail page shows the Configuration
  link, GET the config page (asserts current content / label /
  "Editing:" path), POST a save (asserts `Saved:` banner + file on
  disk matches).
* Web: `config_page_for_an_unknown_component_renders_typed_error` —
  the unknown-id page renders the typed error message.
* Web + API: `config_surface_is_owner_only_on_web_and_api` — an admin
  gets 403 from both API endpoints and a "Only an Owner …" page on
  the web; the config file remains untouched.
* API: `config_api_reads_and_writes_a_managed_component` — owner
  GET/POST happy path, file on disk matches.

The validation-failure restore path is covered at the app layer
through the `EventuallyFailingValidate` package manager; the
integration test server uses `FakePackageManager` whose `validate`
returns `Ok`, so a second-tier restore assertion is not added to the
integration suite (the contract is fully proven at the unit level).

## Follow-ups (not in this change)

* Auto-reload: a per-component reload command (`nginx -t && nginx -s
  reload`, `systemctl reload fail2ban`, ...) executed after a
  successful save, gated by an `allow_reload` manifest flag.
* Multi-file editing: expose `files[1..]` as additional tabs/fields.
* Moving the manifest into the signed catalog once it gains registry
  support for component identity ("software we manage").
