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

### Requirement: Spam Scoring and Spam-Folder Routing

The system SHALL score inbound mail with SpamAssassin or Rspamd and SHALL
route messages above a per-domain score threshold into the mailbox's Spam
folder. Spam scores and headers SHALL be used for routing only; message
bodies SHALL never be written to logs.

#### Scenario: High-score mail routed to Spam

- **WHEN** an inbound message to domain `d1` scores at or above the
        configured `score_threshold`
- **THEN** the message is delivered to the recipient mailbox's `Spam`
        folder and an audit `SpamEvaluated{score}` records the score
        only.

#### Scenario: Clean mail delivered to inbox

- **WHEN** an inbound message scores below `score_threshold`
- **THEN** the message is delivered to the inbox and no spam routing
        occurs.

#### Scenario: Bodies never logged

- **WHEN** any spam-evaluation audit event is written
- **THEN** the event contains the score and headers summary only, never
        the message body.

### Requirement: Greylisting

The system SHALL support greylisting at the domain level; the first seen
(source, destination) tuple SHALL be deferred (SMTP 450) and accepted on
a subsequent retry after a short deferral window.

#### Scenario: First-seen deferred

- **WHEN** a message arrives from a never-before-seen source for domain
        `d1` with greylisting enabled
- **THEN** the message is deferred with `450` and a `GreylistEntry` is
        recorded.

#### Scenario: Retry accepted

- **WHEN** the same (source, destination) retries after the deferral
        window
- **THEN** the message is accepted.

### Requirement: Sieve Filter Management

The system SHALL let an authorised caller manage a Pigeonhole-style Sieve
script per mailbox. The script SHALL be compiled before activation and
SHALL be rejected if it exceeds the size cap or fails to compile.

#### Scenario: Valid script activated

- **WHEN** an Owner `PUT /mail/mailboxes/{m1}/filters` with a valid
        Sieve script under the size cap
- **THEN** the script compiles, is stored active, and dovecot reloads.

#### Scenario: Oversize script rejected

- **WHEN** the script exceeds the size cap (e.g. 64 KiB)
- **THEN** the request is rejected with `MailFilterError::ScriptTooLarge`.

### Requirement: Per-Mailbox Autoresponder

The system SHALL let an authorised caller configure a plaintext
autoresponder per mailbox with an optional active window
(`starts_at`, `ends_at`). The autoresponder SHALL NOT execute any
embedded content.

#### Scenario: Active autoresponder replies

- **WHEN** mail arrives for mailbox `m1` whose autoresponder is enabled
        and within its active window
- **THEN** a single autoresponse is sent per sender and an audit
        `AutoResponderSent{mailbox}` is recorded.

#### Scenario: Outside window no reply

- **WHEN** the current time is outside the configured window
- **THEN** no autoresponse is sent.

### Requirement: Mail Forwarding and Aliases

The system SHALL let an authorised caller manage per-domain forwarders
mapping a local source (or `*` wildcard) to a destination address, and
SHALL detect and reject forwarding loops.

#### Scenario: Forwarder created

- **WHEN** an Owner `PUT /mail/domains/{d1}/forwarders` with a valid
        forwarder
- **THEN** mail to the source is delivered to the destination.

#### Scenario: Loop rejected

- **WHEN** a forwarder would create a delivery loop
- **THEN** the request is rejected with `MailFilterError::ForwardLoop`.

### Requirement: Catch-All Alias

The system SHALL let an authorised caller configure a catch-all alias
per domain that routes otherwise-undeliverable local addresses to a
destination.

#### Scenario: Catch-all delivers

- **WHEN** mail arrives for an unknown local address on domain `d1` with
        a configured catch-all
- **THEN** the message is delivered to the catch-all destination.

### Requirement: Mailing Lists

The system SHALL provide mailman-style mailing lists per domain with
optional moderation and subscriber management; posts from non-members to
a moderated list SHALL be held.

#### Scenario: Member post delivered

- **WHEN** a subscriber posts to list `l1`
- **THEN** the message is delivered to all subscribers.

#### Scenario: Moderated non-member post held

- **WHEN** a non-member posts to moderated list `l1`
- **THEN** the message is held for moderation and not delivered.
