## ADDED Requirements

### Requirement: Mailbox Filter HTTP Surface

The system SHALL expose Sieve filter management for a mailbox over
HTTP and CLI, enforcing the existing size cap and compile gate; an
unauthorised caller SHALL receive 403 without revealing whether the
mailbox exists.

#### Scenario: Script round-trip

- **WHEN** an Owner `PUT`s a valid script under the cap to
        `/api/v1/mail/mailboxes/{m1}/filters` and then `GET`s it
- **THEN** the stored active script is returned byte-identical.

#### Scenario: Oversize rejected over HTTP

- **WHEN** the script exceeds the size cap
- **THEN** the response is 422 with code `script_too_large`.

### Requirement: Autoresponder Surface

The system SHALL expose autoresponder configuration (subject, body,
optional window) per mailbox over HTTP, CLI, and web; window inversion
SHALL be rejected at the surface with the domain error mapped to 422.

#### Scenario: Configure and clear

- **WHEN** an Owner sets a valid autoresponder then deletes it
- **THEN** GET reports no autoresponder between the two calls and one
        exists after the first call.

#### Scenario: Inverted window rejected

- **WHEN** `ends_at` precedes `starts_at`
- **THEN** the response is 422 and nothing is persisted.

### Requirement: Forwarder and Catch-All Surfaces

The system SHALL expose per-domain forwarder CRUD and catch-all
configuration over HTTP, CLI, and web, reusing loop detection;
responses SHALL list forwarders in stable order.

#### Scenario: Forwarder listed after create

- **WHEN** a forwarder is created via `PUT /mail/domains/{d1}/forwarders`
- **THEN** a subsequent GET lists exactly that forwarder.

#### Scenario: Loop surfaced as 422

- **WHEN** a mutation would create a forwarding loop
- **THEN** the response is 422 with code `forward_loop`.

### Requirement: Mailing List Surface

The system SHALL expose mailing-list creation, deletion, moderation
toggle, and subscriber management per domain over HTTP, CLI, and web.

#### Scenario: Subscriber management

- **WHEN** an Owner adds address `a@b.c` to list `l1` then lists
        members
- **THEN** `a@b.c` appears exactly once.

### Requirement: Outbound Queue Visibility

The system SHALL report a real outbound mail queue snapshot (depth,
oldest deferred timestamp, health) per domain through a read-only MTA
port; when the MTA cannot be queried the snapshot SHALL degrade to
`health = "unknown"` instead of failing the request or panicking.

#### Scenario: Queue depth reported

- **WHEN** the MTA port returns depth 3 with oldest deferred at T
- **THEN** `GET /api/v1/mail/domains/{d1}/queue` returns
        `{queue_depth: 3, oldest_deferred_at: T, health: "ok"}`.

#### Scenario: MTA unavailable degrades

- **WHEN** the queue query fails
- **THEN** the endpoint returns 200 with `health: "unknown"` and an
        audit event records the failure reason without message content.
