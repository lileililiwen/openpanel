# Tasks: Add Sites Management

## 1. Domain Layer

- [ ] 1.1 Create `crates/openpanel-domain/src/sites/mod.rs` re-exporting
      `Site`, `SiteStatus`, `SiteError`, `SiteRepository`
- [ ] 1.2 Create `crates/openpanel-domain/src/sites/site.rs` with `Site`
      aggregate (`new`, `validate_domain`, `add_alias`,
      `enable`/`disable`, `change_owner`, `restore`)
- [ ] 1.3 Create `crates/openpanel-domain/src/sites/status.rs` with
      `SiteStatus` enum (`Active`, `Disabled`)
- [ ] 1.4 Create `crates/openpanel-domain/src/sites/error.rs` with
      `SiteError` variants: `InvalidDomain`, `DuplicateDomain`,
      `InvalidAlias`, `InvalidDocumentRoot`, `NotFound`, `Forbidden`,
      `NginxTest(String)`, `Io(String)`, `Persistence(String)`
- [ ] 1.5 Create `crates/openpanel-domain/src/sites/repository.rs` with
      `SiteRepository` trait (insert, find_by_id, find_by_domain,
      list_all, list_by_owner, update_status, update_owner,
      update_aliases, delete, count)
- [ ] 1.6 Re-export `sites::*` from `crates/openpanel-domain/src/lib.rs`

## 2. Application Layer — Service & Repository

- [ ] 2.1 Create `crates/openpanel-app/src/sites/mod.rs` re-exporting
      the module's public surface
- [ ] 2.2 Create `crates/openpanel-app/src/sites/repo.rs` with
      `SqliteSiteRepository` (implements `SiteRepository`)
- [ ] 2.3 Create `crates/openpanel-app/src/sites/service.rs` with
      `SitesService::new(repo, audit, generator)` and methods:
      `create_site`, `list_sites`, `get_site`, `delete_site`,
      `enable_site`, `disable_site`, `change_owner`
- [ ] 2.4 Add RBAC check helper that takes `&User` and decides access
- [ ] 2.5 Add `users` crate dependency to `openpanel-app/Cargo.toml` for
      passwd lookup

## 3. nginx Config Generator

- [ ] 3.1 Create `crates/openpanel-app/src/sites/nginx.rs` with
      `NginxConfigGenerator::new(paths: NginxPaths)` and
      `render(site: &Site) -> String`
- [ ] 3.2 Implement `apply(&str) -> Result<()>` that writes the rendered
      config, runs `nginx -t`, and on failure restores the prior state
- [ ] 3.3 Implement `disable(&str) -> Result<()>` that moves config to
      `disabled/` and reloads
- [ ] 3.4 Implement `remove(&str) -> Result<()>` that deletes the config
      and reloads
- [ ] 3.5 Implement `reload() -> Result<()>` that runs
      `nginx -s reload` after a successful `nginx -t`
- [ ] 3.6 Add unit tests for `render` (snapshot test against a known
      fixture for example.com)

## 4. Document Root Provisioning

- [ ] 4.1 Create `crates/openpanel-app/src/sites/document_root.rs` with
      `provision(root: &Path, owner_uid: u32, owner_gid: u32) -> Result<()>`
      that creates the directory and a placeholder `index.html`
- [ ] 4.2 Implement owner UID lookup via `users::get_user_by_name`
- [ ] 4.3 Add unit test for placeholder content rendering

## 5. Migrations & Module Wiring

- [ ] 5.1 Create `crates/openpanel-app/src/migrations/sites/V001__init.sql`
      defining `sites` table (id, owner_id FK, primary_domain unique,
      aliases JSON, document_root, php_enabled, php_version,
      status TEXT, created_at, updated_at, created_by, modified_by)
- [ ] 5.2 Add `pub const SITES_V001: &str = include_str!(...)` to
      `crates/openpanel-app/src/migrations/mod.rs`
- [ ] 5.3 Create `crates/openpanel-app/src/sites/module.rs` with
      `SitesModule::new(ctx)` returning a struct that implements
      `openpanel_core::Module`
- [ ] 5.4 Wire `SitesModule::migrations()` to return the migration vec
- [ ] 5.5 Re-export `SitesModule` from `crates/openpanel-app/src/lib.rs`

## 6. HTTP Routes

- [ ] 6.1 Create `crates/openpanel-api/src/dto/site.rs` with
      `CreateSiteRequest`, `SiteDto`, `PatchSiteRequest`, `SitesListDto`
- [ ] 6.2 Create `crates/openpanel-api/src/routes/sites.rs` with
      `pub fn router(svc: Arc<SitesService>) -> Router`
- [ ] 6.3 Implement handlers: `create_site`, `list_sites`,
      `get_site`, `delete_site`, `enable_site`, `disable_site`,
      `patch_site`
- [ ] 6.4 Add RBAC: `POST /sites` requires `RequireAdmin` (or owner);
      `DELETE` requires owner OR admin-of-owner; `GET` is scoped
- [ ] 6.5 Implement `IntoResponse for SiteError` mapping in
      `crates/openpanel-api/src/error.rs`
- [ ] 6.6 Update `crates/openpanel-api/src/router.rs::build_router` to
      accept `sites: Arc<SitesService>` and nest under
      `/api/v1/sites`

## 7. CLI

- [ ] 7.1 Add `SiteCommand` enum and `site` subcommand in
      `crates/openpanel-cli/src/commands.rs`
- [ ] 7.2 Implement `handlers::create_site`, `list_sites`,
      `delete_site`, `enable_site`, `disable_site` that call
      `SitesService` directly (reuse the composition-root bootstrap)
- [ ] 7.3 Wire the new subcommand in
      `crates/openpanel-cli/src/main.rs`

## 8. Composition Root

- [ ] 8.1 Update `openpanel-cli/src/handlers.rs::serve` to also build
      `SitesModule` and register its migrations
- [ ] 8.2 Pass `sites` service into `build_router(...)`
- [ ] 8.3 Confirm `cargo build --workspace --release` succeeds
- [ ] 8.4 Confirm `cargo test --workspace` passes

## 9. Validation

- [ ] 9.1 `openspec validate add-sites-management` returns valid
- [ ] 9.2 Update `openspec/specs/sites/spec.md` after archive
- [ ] 9.3 Update root `README.md` with sites-management usage example
- [ ] 9.4 Manual smoke: `openpanel site create --domain test.local
      --owner admin` followed by `curl GET /api/v1/sites` returns the
      site
- [ ] 9.5 Manual smoke: `openpanel site delete --id <uuid>` removes the
      config and reloads nginx
- [ ] 9.6 Commit + archive via OpenSpec