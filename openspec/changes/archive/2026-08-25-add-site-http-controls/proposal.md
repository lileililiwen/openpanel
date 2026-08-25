# Add Site HTTP controls

## Why

Every classic panel we benchmark against — cPanel (Error Pages,
Redirects, Directory Privacy, Hotlink Protection), Plesk (error
documents, password-protected directories), Froxlor (directory
protection, web-forwarders), CloudPanel (Basic Auth sites, IP & bot
blocking) and KeyHelp — ships per-site HTTP controls. OpenPanel's nginx
renderer (`crates/openpanel-app/src/sites/nginx.rs`) hardcodes
`index index.html index.htm` and only emits the force-HTTPS redirect;
the WAF rule set has no per-site client-IP CIDR kind. This is the
largest single baseline-parity gap.

## What Changes

- Per-site **custom error pages** (404/500-class overrides rendered as
  nginx `error_page` directives).
- Ordered per-site **redirect rules** (path prefix → destination,
  301/302/307/308) with loop detection.
- **Protected directories**: htpasswd-style basic auth on a path
  prefix; credentials hashed with the existing bcrypt primitives and
  stored as ciphertext.
- **Hotlink protection** (referer allow-list with default deny/allow).
- Per-site **client IP allow/deny CIDR rules**.
- Per-site **MIME type overrides** and a **directory-index policy**
  (index order + autoindex toggle).
- Pure, stable nginx snippet compilation following the WAF pattern;
  REST + CLI + web surfaces; audit events on every mutation.

## Capabilities

### New Capabilities

- `site-http-controls`: per-site HTTP behaviour controls compiled into
  the site's nginx vhost by the existing sites renderer.

## Impact

- Domain: `ErrorPageOverride`, `RedirectRule`, `ProtectedDir`,
  `HotlinkPolicy`, `ClientIpRule`, `MimeOverride`, `IndexPolicy` under
  `crates/openpanel-domain/src/site_http_controls/`.
- App: `SiteHttpControlsService` + SQLite repo; snippet compiler
  reuses the pure-compilation pattern of
  `crates/openpanel-app/src/waf/`; output is spliced by
  `crates/openpanel-app/src/sites/nginx.rs::render_full`.
- API/CLI/web: `/api/v1/sites/{id}/http/*` routes,
  `openpanel site http …` commands, Sites UI tabs.
- Security: basic-auth secrets hashed (never plaintext at rest), no
  secret material in logs or responses.
- Coupling: builds on `sites`; reuses `waf` compilation conventions.

## Non-goals

- No full vhost free-text editing (CloudPanel-style vhost editor) —
  separate future change.
- No regex capture-group rewrites beyond prefix matching.
- No per-site transport tuning (HTTP/3, gzip/brotli, HSTS) — tracked
  in `docs/competitive-gap-analysis.md` backlog item 1.
