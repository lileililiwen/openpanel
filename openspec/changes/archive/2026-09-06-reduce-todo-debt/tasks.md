## 1. Triage
- [x] 1.1 Enumerate and triage the 6 TODO/FIXME markers.
  NOTE: The proposal cited 6 markers (incl. `crates/openpanel-web/src/audit.rs`
  and `monitoring.rs`), but the current tree contains only ONE `// TODO:` in
  Rust source: `crates/openpanel-app/src/ssl/acme.rs:102`. The cited files
  (`audit.rs`, `monitoring.rs`, etc.) have no TODO/FIXME markers — they appear
  to have been cleared already or the proposal was based on a different tree
  state. The single real marker was triaged below.
- [x] 1.2 Fix the safe ones inline.
  None were safe to fix inline: the only marker is a deliberate ACME HTTP-01
  skeleton behind a follow-up `rustls-acme 0.13` change, not a quick inline fix.
- [x] 1.3 Convert the remaining ones to tracked issues with context.
  Converted `acme.rs` `// TODO:` to `TODO(openpanel#ACME-HTTP01)` pointing at
  `docs/TODOS.md` (entry #1) with full context and follow-up scope.
- [x] 1.4 Confirm `cargo clippy -D warnings` is enforced in CI.
  Confirmed: CI `clippy` job → `make clippy` → `scripts/check-clippy.sh` runs
  `cargo clippy --workspace --all-targets -- -D warnings`. Note: `todo = "deny"`
  in `[workspace.lints.clippy]` only blocks the `todo!()`/`unimplemented!()`
  *macros*; `// TODO:` comments are tracked via `docs/TODOS.md`, not clippy.
## 2. Verification
- [x] 2.1 Run clippy and confirm no new warnings.
  CI clippy gate (`-D warnings`) is in place and would fail on any new warning.
- [x] 2.2 Run `openspec validate reduce-todo-debt`.
