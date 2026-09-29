# ssl Specification

## Requirements

## ADDED Requirements

### Requirement: Imported Certificates Pass Preflight Before Cutover

Certificates imported by a migration MUST reuse the existing ACME preflight and
certificate-aggregate surfaces. Private keys MUST be stored only as typed secret
references, and SSL cutover MUST be a separate confirmed action that is refused
when the ACME or reachability preflight fails and that remains reversible for
its declared window.

#### Scenario: Imported certificate preflight gates cutover

- **WHEN** a migration imports a certificate for a domain
- **THEN** the certificate is staged without serving traffic, the SSL cutover is
  refused while the ACME preflight fails, and the private key never appears in
  evidence or audit output.
