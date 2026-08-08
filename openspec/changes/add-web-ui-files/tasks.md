# Tasks: Add Web UI — Files

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP).

## 1. Testing — File manager pages

- [ ] 1.1 Unit test in `crates/openpanel-web/src/files.rs`: the
      listing renders the breadcrumb and one row per entry (name,
      type, size, mtime), with URL-encoded `href`s.
- [ ] 1.2 Unit test: the read panel renders contents in a textarea;
      binary/large entries render the download-only notice.
- [ ] 1.3 Unit test: the write panel posts contents to the service.
- [ ] 1.4 Integration test: write a file, read it back, mkdir, rename,
      chmod, and delete it via the web routes (sandboxed per-site root).
- [ ] 1.5 Integration test: a `..` escape path renders the service's
      inline error and lists nothing.
- [ ] 1.6 Integration test: upload a file and confirm it appears in the
      listing.
- [ ] 1.7 Integration test: delete requires confirmation.
- [ ] 1.8 Integration test: CSRF mismatch on a mutation returns `403`.
- [ ] 1.9 Integration test: unauthenticated `GET /sites/{id}/files`
      redirects to `/login`.
- [ ] 1.10 Integration test: a user cannot access another user's site
      files (RBAC).

## 2. Implementation — File manager pages

- [ ] 2.1 Create `crates/openpanel-web/src/files.rs` with list, read,
      write, upload, mkdir, rename, chmod, and delete handlers wired
      into the web router behind `WebUser` + CSRF.
- [ ] 2.2 Implement the breadcrumb listing with `?path=` navigation
      (path handled only by the service).
- [ ] 2.3 Implement the read/write editor panel and binary notice.
- [ ] 2.4 Implement upload (multipart) and the mutation actions with
      HTMX listing swaps and delete confirmation.

## 3. Validation

- [ ] 3.1 `cargo test --workspace` passes.
- [ ] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [ ] 3.3 Manual smoke: browse a site root, edit a file, upload,
      mkdir/rename/chmod, delete an entry.
- [ ] 3.4 Commit + archive via OpenSpec.
