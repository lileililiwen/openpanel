# Tasks: Add Sites Management

## 1. Domain Layer

- [x] 1.1 Create `crates/openpanel-domain/src/sites/mod.rs` re-exporting
      `Site`, `SiteStatus`, `SiteError`, `SiteRepository`
- [x] 1.2 Create `crates/openpanel-domain/src/sites/site.rs` with `Site`
      aggregate (`new`, `validate_domain`, `add_alias`,
      `enable`/`disable`, `change_owner`, `restore`)
- [x] 1.3 Create `crates/openpanel-domain/src/sites/status.rs` with
      `SiteStatus` enum (`Active`, `Disabled`)
- [x] 1.4 Create `crates/openpanel-domain/src/sites/error.rs` with
      `SiteError` variants
- [x] 1.5 Create `crates/openpanel-domain/src/sites/repository.rs` with
      `SiteRepository` trait
- [x] 1.6 Re-export `sites::*` from `crates/openpanel-domain/src/lib.rs`

## 2. Application Layer — Service & Repository

- [x] 2.1 Create `crates/openpanel-app/src/sites/mod.rs`
- [x] 2.2 Create `crates/openpanel-app/src/sites/repo.rs` with
      `SqliteSiteRepository`
- [x] 2.3 Create `crates/openpanel-app/src/sites/service.rs` with
      `SitesService::new(...)` and methods
- [x] 2.4 Add RBAC check helper that takes `&User`
- [x] 2.5 Add `libc` (was: `users`) dep for /etc/passwd lookup

## 3. nginx Config Generator

- [x] 3.1 Create `crates/openpanel-app/src/sites/nginx.rs` with
      `NginxConfigGenerator::new(paths)` and `render(site)`
- [x] 3.2 Implement `apply(&str)` with nginx -t (graceful if nginx missing)
- [x] 3.3 Implement `disable(&str)` with nginx -t (graceful if nginx missing)
- [x] 3.4 Implement `remove(&str)`
- [x] 3.5 Implement `reload()` (graceful when nginx missing)
- [x] 3.6 Unit tests for `render` (server_name, root, php index)

## 4. Document Root Provisioning

- [x] 4.1 Create `crates/openpanel-app/src/sites/document_root.rs` with
      `provision(root, owner_name, site_name)`
- [x] 4.2 Implement owner UID lookup via `/etc/passwd` parsing (no extra
      dep)
- [x] 4.3 Unit test for placeholder content + lookup

## 5. Migrations & Module Wiring

- [x] 5.1 Create `crates/openpanel-app/src/migrations/sites/V001__init.sql`
- [x] 5.2 Add `pub const SITES_V001` to migrations/mod.rs
- [x] 5.3 Create `crates/openpanel-app/src/sites/module.rs` with
      `SitesModule::new(ctx)`
- [x] 5.4 Wire `SitesModule::migrations()`
- [x] 5.5 Re-export `SitesModule` from `crates/openpanel-app/src/lib.rs`

## 6. HTTP Routes

- [x] 6.1 Create `crates/openpanel-api/src/dto/site.rs`
- [x] 6.2 Create `crates/openpanel-api/src/routes/sites.rs` with
      `pub fn router(svc: Arc<SitesService>) -> Router`
- [x] 6.3 Implement handlers (create_site, list_sites, get_site,
      delete_site, enable_site, disable_site, patch_site)
- [x] 6.4 RBAC: enforced at service layer (Owner / Admin / User)
- [x] 6.5 `IntoResponse for SiteError` mapped in `routes/sites.rs`
- [x] 6.6 Update `crates/openpanel-api/src/router.rs::build_router` to
      nest `/api/v1/sites` (now takes `sites: Arc<SitesService>`)

## 7. CLI

- [x] 7.1 Add `SiteCommand` enum and `site` subcommand in commands.rs
- [x] 7.2 Implement handlers::create_site, list_sites, delete_site,
      enable_site, disable_site
- [x] 7.3 Wire the new subcommand in main.rs

## 8. Composition Root

- [x] 8.1 Update `openpanel-cli/src/handlers.rs::serve` to also build
      `SitesModule` and register its migrations
- [x] 8.2 Pass `sites` service into `build_router(...)`
- [x] 8.3 `cargo build --workspace --release` succeeds (debug used during dev)
- [x] 8.4 `cargo test --workspace` passes — 21 tests

## 9. Validation

- [x] 9.1 `openspec validate add-sites-management` returns valid
- [x] 9.2 Update `openspec/specs/sites/spec.md` after archive
- [x] 9.3 Update root `README.md` with sites-management usage
- [x] 9.4 Manual smoke: identity login + `GET /api/v1/sites` returns `[]` (200)
- [x] 9.5 Manual smoke: `GET /api/v1/sites/<bad-uuid>` returns 404
      `not_found`
- [x] 9.6 Commit + archive via OpenSpec

## Notes

- **nginx is optional in v0.1**: if the nginx binary is not present in
  the environment, the generator writes config files and logs a warning
  instead of running `nginx -t` / `nginx -s reload`. This keeps
  development environments (CI, dev machines without nginx) working.
- **/etc/passwd lookup** uses a small in-tree parser to avoid adding a
  `users` crate dependency. Parses 7 colon-separated fields.
- **RBAC** is enforced at the service layer; the HTTP layer just passes
  the authenticated user through. CLI handlers resolve the `admin` user
  as the caller.