## ADDED Requirements

### Requirement: Runtime Environment Variables

An authorised caller SHALL manage an ordered set of environment
variables per runtime. Keys SHALL match `[A-Za-z_][A-Za-z0-9_]*`
(≤128 chars), reserved keys SHALL be rejected, and sets beyond 128
variables or 64 KiB total SHALL be rejected at validation.

#### Scenario: Values reach the process

- **WHEN** an Owner sets `LOG_LEVEL=debug` and restarts the runtime
- **THEN** the application process observes `LOG_LEVEL=debug` in its
        environment.

#### Scenario: Reserved key rejected

- **WHEN** a caller defines `PATH`
- **THEN** validation fails with `EnvError::ReservedKey` and nothing
        is persisted.

### Requirement: Secret Variables

Variables flagged as secrets SHALL be stored only as AES-256-GCM
ciphertext, SHALL be written into a 0600 environment file inside the
runtime's jail referenced by `EnvironmentFile=`, and SHALL never be
returned by any surface, written to logs, or embedded in the supervisor
unit text.

#### Scenario: Write-only secret

- **WHEN** a secret variable is stored and later listed via API, CLI,
        or web
- **THEN** responses contain its key and `secret: true` but never its
        value.

#### Scenario: Unit stays clean

- **WHEN** any runtime with secret variables is rendered
- **THEN** the supervisor unit contains no secret plaintext; secrets
        appear only in the jailed 0600 env file.

### Requirement: Pending-Restart Semantics

Environment changes SHALL take effect on the next runtime restart;
surfaces SHALL expose a pending-vs-applied indicator that clears when
the existing restart action completes.

#### Scenario: Pending cleared by restart

- **WHEN** an Owner updates the environment then restarts the runtime
- **THEN** the pending indicator is false afterwards and the process
        runs with the new set.
