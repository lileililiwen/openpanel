# Add Terraform provider and SDK

## Why

OpenPanel already maintains a generated OpenAPI JSON (`openapi.rs`), but
there is **no client SDK and no Terraform provider** — the only way to
drive the panel programmatically is hand-rolled HTTP calls. Competitors
and modern infra teams expect first-class IaC: typed clients in common
languages and a Terraform provider for declaring sites, users, DNS, and
backups as code. This change adds an `iac` bounded context that
generates a typed client SDK from the OpenAPI spec and publishes a
Terraform provider, with CI keeping them in sync with the API contract.

## What Changes

- New bounded context `iac` describing the contract for a generated,
  typed client SDK (Rust / Go / TypeScript) and a Terraform provider
  covering sites, users, DNS, and backups.
- Artifacts (produced by the implementation, tracked here as the
  contract): SDK packages for Rust/Go/TS and a Terraform provider
  repository scaffold.
- A CI contract that regenerates the SDK and provider from `openapi.rs`
  and fails on drift between the API and the generated surfaces.
- The spec stays focused on the **contract** — what resources/operations
  the SDK and provider expose and their sync guarantee — not the
  codegen internals.

## Capabilities

### New Capabilities

- `iac`: a typed client SDK (Rust/Go/TS) and a Terraform provider for
  sites/users/dns/backups, generated from the OpenAPI spec and kept in
  sync by CI.

## Impact

- Domain: `SdkPackage`, `TerraformProvider`, `ApiContract` (descriptor).
- App/CI: `CodegenContract` verifying SDK/provider against `openapi.rs`.
- API/CLI/web: no new panel endpoints; consumes the existing REST API.
- Security: SDK/provider authenticate via existing API tokens; the
  provider requests only the scopes its resources need.
- Coupling: depends on `api` for the REST surface and on `openapi.rs`
  as the single source of truth for the generated contract.
