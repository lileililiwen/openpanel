# databases Specification

## ADDED Requirements

### Requirement: Database List Row Actions

The `GET /databases` list MUST expose per-row actions reusing
the existing `DatabasesService` and RemoteAccess enforcement:
password view/copy (one-time panel only, never inline),
quota and backup-count columns, `.sql` / `.zip` import
upload, permission editor (Local / Everyone / Specified IP,
with the `allow_any_source` owner opt-in and
`global_access_locked` refusal), Tools (repair / optimize /
convert) with confirmation, and a phpMyAdmin SSO deep-link
via the existing `admin-tool` single-use mint. Every mutation
MUST validate CSRF and caller permission.

#### Scenario: Password view stays one-time

- **WHEN** an authorized caller reveals a database password
- **THEN** the plaintext renders exactly once in the password
  panel and the list HTML contains no credential material.

#### Scenario: Open-by-default permission refused

- **WHEN** a caller submits Everyone without the explicit
  owner opt-in
- **THEN** the request is rejected with
  `global_access_locked` and the ACL is unchanged.

#### Scenario: Tools require confirmation

- **WHEN** an authorized caller opens a Tools action
- **THEN** repair / optimize / convert each render through
  the layer-confirm gate before dispatch.

### Requirement: Sync-All And Get-From-Server Adopt Flow

The databases surface MUST offer Sync-all and Get-from-server:
a read-only comparison listing server-side databases missing
from the panel, with an adopt action that registers panel
metadata only. Adopt MUST NOT import credentials and MUST
validate CSRF and caller permission.

#### Scenario: Adopt registers metadata only

- **WHEN** an authorized caller adopts a server-side database
  missing from the panel
- **THEN** the panel lists it with its encrypted credential
  lifecycle and no server credential bytes are persisted.
