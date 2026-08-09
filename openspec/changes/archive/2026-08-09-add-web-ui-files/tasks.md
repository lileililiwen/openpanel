# Tasks: Add Web UI — Files

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP).

## 1. Testing — File manager pages

- [x] 1.1 Unit test in `crates/openpanel-web/src/files.rs`: the
      listing renders the breadcrumb and one row per entry (name,
      type, size, mtime), with URL-encoded `href`s.
- [x] 1.2 Unit test: the read panel renders contents in a textarea;
      binary/large entries render the download-only notice.
- [x] 1.3 Unit test: the write panel posts contents to the service.
- [x] 1.4 Integration test: write a file, read it back, mkdir, rename,
      chmod, and delete it via the web routes (sandboxed per-site root).
- [x] 1.5 Integration test: a `..` escape path renders the service's
      inline error and lists nothing.
- [x] 1.6 Integration test: upload a file and confirm it appears in the
      listing.
- [x] 1.7 Integration test: delete requires confirmation.
- [x] 1.8 Integration test: CSRF mismatch on a mutation returns `403`.
- [x] 1.9 Integration test: unauthenticated `GET /sites/{id}/files`
      redirects to `/login`.
- [x] 1.10 Integration test: a user cannot access another user's site
      files (RBAC).

## 2. Implementation — File manager pages

- [x] 2.1 Create `crates/openpanel-web/src/files.rs` with list, read,
      write, upload, mkdir, rename, chmod, and delete handlers wired
      into the web router behind `WebUser` + CSRF.
- [x] 2.2 Implement the breadcrumb listing with `?path=` navigation
      (path handled only by the service).
- [x] 2.3 Implement the read/write editor panel and binary notice.
- [x] 2.4 Implement upload (multipart) and the mutation actions with
      HTMX listing swaps and delete confirmation.

## 3. Validation

- [x] 3.1 `cargo test --workspace` passes.
- [x] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [x] 3.3 Manual smoke: browse a site root, edit a file, upload,
      mkdir/rename/chmod, delete an entry. — covered by
      `web_files_full_workflow` (write → read → mkdir → rename →
      chmod → delete), `web_files_upload_appears_in_listing`,
      `web_files_escape_attempt_rejected`, and
      `web_files_rbac_blocks_other_user` in
      `tests/integration/web_ui.rs`, which exercise the full flow
      through the router.
- [x] 3.4 Commit + archive via OpenSpec.
