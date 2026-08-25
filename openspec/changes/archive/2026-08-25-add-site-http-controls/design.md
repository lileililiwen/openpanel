# Add Site HTTP controls — Design

## Explore & Reuse

- **nginx rendering pipeline**: `crates/openpanel-app/src/sites/nginx.rs`
  (`render_full`, lines 143–287) is the only place vhost text is
  produced; new directives are spliced there — no second renderer.
- **Pure snippet compilation**: `crates/openpanel-app/src/waf/`
  ("Typed Rule Set / Snippet Compilation is Pure and Stable" in
  `openspec/specs/waf/spec.md`) is the pattern to copy: typed rules →
  deterministic nginx text, unit-testable without I/O.
- **Secret hashing**: identity's bcrypt-backed `Password` primitives
  (`crates/openpanel-domain/src/identity/`, workspace dep `bcrypt =
  "0.17"`) hash htpasswd entries; AES-256-GCM ciphertext layout from
  `crypto.rs` for any stored material.
- **Path validation**: `openpanel_domain::files::Path` VO guards user
  supplied paths for error-page documents.
- **Audit**: `openpanel_core::AuditService` for every mutation.
- **Module wiring**: new `SiteHttpControlsModule` registered via
  `ModuleRegistry::register` (`crates/openpanel-core/src/module.rs:94`);
  routes under `crates/openpanel-api/src/routes/site_http_controls.rs`;
  CLI dispatch in `crates/openpanel-cli/src/handlers.rs`.

## Models

```rust
pub struct ErrorPageOverride { site_id, status: u16, document_path: Path }
pub struct RedirectRule      { site_id, ordinal, source_prefix, destination, status: RedirectStatus }
pub struct ProtectedDir      { site_id, path_prefix, realm, accounts: Vec<BasicAuthAccount> }
pub struct BasicAuthAccount  { name, bcrypt_hash: String }   // never plaintext at rest
pub struct HotlinkPolicy     { site_id, allowed_referers: Vec<String>, default_deny: bool }
pub struct ClientIpRule      { site_id, ordinal, cidr: IpCidr, effect: Allow|Deny }
pub struct MimeOverride      { site_id, extension, mime_type }
pub struct IndexPolicy       { site_id, order: Vec<String>, autoindex: bool }
```

Validation: statuses ∈ {301,302,307,308}; CIDR via existing ip-allocation
CIDR parsing; redirect loop = source prefix that is also a destination
prefix of an earlier rule → rejected.

## Rendering flow

```
render_full(site, base_ctx):
    ...existing...
    splice SiteHttpControlsRenderer.render(controls):
        index policy        -> `index` + optional `autoindex`
        error pages         -> `error_page <status> <path>;`
        redirects           -> ordered `location =/prefix { return <status> <dest>; }`
        protected dirs      -> `location ^~ prefix { auth_basic; auth_basic_user_file; }`
        hotlink             -> `valid_referers` + `if $invalid_referer { return 403 }`
        ip rules            -> `allow/deny <cidr>;` inside site location
        mime overrides      -> `types { ... }` block
    output MUST be byte-stable for equal input (property test)
```

htpasswd file written to `SslPaths`-style 0600 dir owned by the site user.

## Endpoints

```
GET/PUT  /api/v1/sites/{id}/http/error-pages
GET/PUT  /api/v1/sites/{id}/http/redirects        (ordered list replace)
POST/DELETE /api/v1/sites/{id}/http/protected-dirs[/{dir}/accounts]
GET/PUT  /api/v1/sites/{id}/http/hotlink
GET/PUT  /api/v1/sites/{id}/http/ip-rules
GET/PUT  /api/v1/sites/{id}/http/mime
GET/PUT  /api/v1/sites/{id}/http/index
```

CLI: `openpanel site http {error-page,redirect,protect,hotlink,ip,mime,index}`.
Web: Sites → "HTTP Controls" tabs (tokens.css forms, three breakpoints).

## Layering

Domain crate gains `site_http_controls` (pure types + validation, zero
I/O). App crate owns repo + renderer + service. API/CLI/web adapters
consume the service. No cross-context edits beyond composition-root
registration.
