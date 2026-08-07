# `openpanel-test-support`

Test fixtures, helpers, and mocks for the OpenPanel workspace.

## What's here

| Item | Purpose |
|---|---|
| `TestDb` | Per-test SQLite database under `/tmp/openpanel-test/<uuid>.db`. Runs all module migrations. `Drop` deletes the file. |
| `TestServer` | Boots the real axum router from `openpanel_api::build_router` on a random port. Sandbox temp dir for nginx configs + document roots. |
| Mocks (`mockall::mock!`) | `MockUserRepo`, `MockSessionRepo`, `MockSiteRepo`, `MockDatabaseRepo`, `MockFileRepo`, `MockAudit`. |

## Quick start

```rust
use openpanel_test_support::TestServer;

#[tokio::test]
async fn login_succeeds() {
    let server = TestServer::new().await;
    let token = server.bootstrap_owner("admin", "correct horse battery staple").await;

    let me = server.client()
        .get(format!("{}/api/v1/identity/me", server.base_url()))
        .bearer_auth(&token)
        .send().await.unwrap();
    assert_eq!(me.status(), 200);
}
```

## Sandbox paths

Tests should never touch `/var/www/` or other real paths. Use
`server.sandbox_path("sub/path")` to get a temp dir path that auto-cleans
on `Drop`:

```rust
let doc_root = server.sandbox_path(&format!("sites/{}/public_html", Uuid::new_v4()));
```

## Test categories

1. **Unit tests** — `#[cfg(test)] mod tests` inside each crate. No I/O.
   Use mocks for repository dependencies.
2. **Property tests** — `proptest!` blocks in domain code for invariants
   (input → rejected, input → accepted).
3. **Integration tests** — `tests/integration/*.rs`. Boots `TestServer`
   and exercises the real router + SQLite + sandboxed filesystem.
4. **CLI E2E tests** — `tests/cli/*.rs`. Spawns `openpanel` as a subprocess
   against a fresh `TestDb`.

## Conventions

- Tests are allowed to use `.unwrap()` / `.expect()` / `panic!`. Production
  code is denied (`clippy.toml`).
- Every `TestServer::new()` is isolated: its own DB file, its own temp
  sandbox, its own port.
- Tests run with `--test-threads=1` to avoid SQLite contention.

## See also

- `openspec/changes/add-tdd-infrastructure` — the full proposal.
- `tests/README.md` — how to run each category.
- `Agents.md` — quality engineering principles.