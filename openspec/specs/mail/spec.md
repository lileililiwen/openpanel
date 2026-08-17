# mail Specification

## Purpose

The mail bounded context covers mail domain lifecycle, mailbox and
alias management, MTA configuration, and quota. After this
refinement, it also owns the DKIM / SPF / DMARC defaults and the
per-mailbox quota value objects that the mail service consumes
when a domain is enabled.

## Requirements

### Requirement: DKIM Keypair

The mail bounded context SHALL model a `DkimKeypair` carrying `domain`, `selector`, `algorithm` (`Rsa2048 | Ed25519`), `private_key_blob` (encrypted at rest), `public_key`, `created_at`, and `rotation_grace_until`. The constructor rejects empty `domain` or `selector`. The pair supports a rotation grace window during which both the old and new keys may sign.

#### Scenario: Constructor rejects empty inputs

- **WHEN** `DkimKeypair::new("", _, _, _, _, _)` is called
- **THEN** the constructor returns `MailError::Invalid`.

#### Scenario: Rotation grace elapsed

- **WHEN** `rotation_grace_until` is in the past
- **THEN** `rotation_grace_elapsed_at(now)` returns `true`.

### Requirement: Per-Mailbox Quota

The mail bounded context SHALL model a `MailboxQuota` (bytes) with constructors that validate a minimum and maximum. The `permits(used, incoming)` helper returns `true` only when the delivery fits within the quota.

#### Scenario: Boundary delivery

- **WHEN** `quota = 100`, `used = 80`, `incoming = 30`
- **THEN** `permits` returns `false`.

### Requirement: Domain Sending Policy

The mail bounded context SHALL model a `DomainSendingPolicy` carrying `outbound_per_minute`, `max_recipients_per_message`, and three `SendingRequirement` flags (`spf`, `dkim`, `dmarc`). The default applied at `enable_domain` requires `SPF` and `DKIM` and soft-fails `DMARC`. The `accepts(passed_spf, passed_dkim, passed_dmarc)` helper returns `Required` when a required check has not passed.

#### Scenario: Required DKIM

- **WHEN** `dkim = Required` and `passed_dkim = false`
- **THEN** `accepts` returns `MailErrorSendingPolicy::Required`.

#### Scenario: Soft-fail DMARC

- **WHEN** `dmarc = SoftFail` and `passed_dmarc = false`
- **THEN** `accepts` returns `Ok(())`.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `MailDkimKeypairGenerated`, `MailDkimRotated`, `MailboxQuotaExceeded`, and `MailSendingPolicyRejected` audit events. The bounded context as archived today owns the typed model and the validation rules.
