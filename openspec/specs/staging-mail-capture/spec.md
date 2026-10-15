# staging-mail-capture Specification

## Purpose

Promoted from the `adopt-openserverpanel-batch` capability set borrowed
from OpenServerPanel. A staging slot must never send real mail to
production recipients — form handlers, contact pages, and cron report
jobs all submit through the webmail path. This capability diverts
outbound mail of a staging slot whose capture toggle is on into an
owner-visible inbox with bodies encrypted at rest, a FIFO retention
cap, and purge-on-destroy semantics, so staging behaves like
production without its blast radius.
## Requirements
### Requirement: Capture Diverts Site Outbound Mail

When mail capture is enabled for a site's active staging slot, the
panel's user-facing send path SHALL store the message in the capture
inbox instead of handing it to the production mail transport, and the
sender receives a success result marked as captured.

#### Scenario: Staging send captured

- **WHEN** a webmail user on a site with capture enabled sends a
  message
- **THEN** no SMTP delivery is attempted and the message appears in
  that site's capture inbox.

#### Scenario: Production path untouched

- **WHEN** capture is disabled for the slot
- **THEN** the send behaves identically to the pre-change flow.

### Requirement: Captured Message Contents

A captured message SHALL record recipient(s), sender, subject,
timestamp, and body; bodies MUST be encrypted at rest with the panel
secret storage and MUST NEVER be returned in audit payloads or logs.

#### Scenario: Read detail

- **WHEN** an authorized owner/admin opens a captured message
- **THEN** the decrypted body and metadata are displayed.

### Requirement: Inbox Visibility and Purge

Owners and admins SHALL list and read the capture inbox per site,
and SHALL be able to purge all messages for a slot. Deletion follows
slot destruction automatically (see `site-staging`).

#### Scenario: Purge all

- **WHEN** an operator purges the inbox of a slot
- **THEN** zero messages remain for that slot.

### Requirement: Bounded Retention

The capture inbox SHALL enforce a per-slot retention cap (default
1000 messages), evicting the oldest captured messages FIFO when
exceeded.

#### Scenario: Cap enforced

- **WHEN** a message is stored into a full inbox
- **THEN** the oldest message is evicted and the stored count stays at
  the cap.
