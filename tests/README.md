# Tests

This directory holds **integration** and **CLI E2E** tests. Unit and
property tests live next to the code they cover (`#[cfg(test)] mod tests`
in each module).

## Layout

```
tests/
├── common/
│   └── mod.rs               # re-exports openpanel_test_support::*
├── integration/             # HTTP API integration tests (single binary)
│   ├── main.rs              #   entry point that wires every module
│   ├── identity.rs          #   /api/v1/identity/*
│   ├── sites.rs             #   /api/v1/sites/*
│   ├── databases.rs         #   /api/v1/databases/*
│   ├── files.rs             #   /api/v1/files/*
│   ├── smoke.rs             #   server boots + every route mounted
│   └── debug_test.rs        #   canary for the new infra
└── cli/                     # CLI E2E tests (separate binaries per file)
    ├── common.rs            #   CliRunner helper
    ├── serve.rs             #   openpanel serve + curl /health
    ├── user.rs              #   create-admin + list
    ├── site.rs              #   nginx-gated
    ├── database.rs          #   mysql-gated
    └── file.rs              #   write + read roundtrip
```

## Running

```bash
# Everything (unit + property + integration)
cargo test --workspace

# Integration only
cargo test -p openpanel --test integration

# A single integration test
cargo test -p openpanel --test integration -- 'sites::sites_list_empty_for_fresh_db'

# With more proptest cases
PROPTEST_CASES=1000 cargo test --workspace

# CLI E2E
cargo test -p openpanel --test cli_serve --test cli_user --test cli_file
```

## Conventions

- Every integration test boots its own `TestServer` (per-test DB, sandbox,
  port). No shared state.
- Tests use `--test-threads=1` to avoid SQLite lock contention. The CI
  script does this automatically.
- Production code is forbidden from `.unwrap()` / `.expect()` /
  `panic!` / `todo!` / `unimplemented!`. Test code is allowed. The
  `clippy.toml` policy enforces this.
- Tests that need external binaries (`mysql`, `nginx`) skip themselves
  with `eprintln!("skipped: ...")` rather than failing.

## Adding a new integration test

1. Pick the right file under `tests/integration/` (`sites.rs`,
   `databases.rs`, etc.) — or create a new one if a new module ships.
2. Import `use crate::common::*;` for the `TestServer` re-exports.
3. `let server = TestServer::new().await;` at the top of the test.
4. Use `server.bootstrap_owner("admin", "...").await` for a token.
5. Use `server.client().get(...).bearer_auth(&token)` for authed
   requests. URLs are **without trailing slashes** (axum `nest` doesn't
   add them).

## See also

- `crates/openpanel-test-support/README.md`
- `openspec/changes/add-tdd-infrastructure/tasks.md`
- `Agents.md` § Quality Engineering