# ssl-production-lifecycle delta

## MODIFIED Requirements

### Requirement: Complete HTTP-01 Issuance

The production ACME adapter MUST complete directory discovery, account/order
creation, HTTP-01 challenge polling, finalization, and certificate retrieval
using the configured rustls-acme version. The adapter bridges the rustls-acme
event stream into the panel's `AcmeHttpServer` via a token lookup hook so the
existing nginx-proxied challenge topology serves validations, and no residual
skeleton/stub error path remains in `RustlsAcmeClient::issue`.

#### Scenario: Staging issuance succeeds

- **WHEN** an authorized caller requests staging issuance for a reachable domain
- **THEN** the certificate is stored as an ACME certificate with staging
  endpoint metadata and encrypted key material.

#### Scenario: Live wire path validated offline

- **WHEN** the adapter runs issuance against a scripted ACME directory
  presenting the full HTTP-01 order flow
- **THEN** the challenge token is answerable on the panel challenge server
  during the flow and the returned `IssuedCert` carries the leaf, chain, and
  private key PEMs with a detected issuer.

#### Scenario: Stalled order is bounded

- **WHEN** the ACME order never leaves processing within the configured poll
  bound
- **THEN** issuance returns a timeout-classified error, the challenge lookup
  hook is detached, and no certificate row is written.
