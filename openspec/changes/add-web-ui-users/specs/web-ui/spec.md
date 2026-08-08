## ADDED Requirements

### Requirement: User List

The `web-ui` SHALL render `GET /users` inside the shell for `Owner` callers: a
table with one row per user (username, email, role, status, created, last
login) and row actions the owner may perform. Non-owner callers SHALL NOT see
the table and SHALL be shown a forbidden message.

#### Scenario: Owner lists users

- **WHEN** an `Owner` requests `/users`
- **THEN** the server renders the user table with all users and their
  metadata.

#### Scenario: Non-owner denied

- **WHEN** a non-owner requests `/users`
- **THEN** the server renders a forbidden message and no user data.

### Requirement: Create User

The UI SHALL provide `GET /users/new` (form) and `POST /users` (create),
collecting username, email, password, and role. The password SHALL NOT be
rendered back or persisted in the page; validation errors SHALL render
inline.

#### Scenario: Creating a user

- **WHEN** an `Owner` submits a valid create form
- **THEN** the user is created and the list re-renders with the new row.

### Requirement: User Actions

The UI SHALL support role change (`PATCH /users/{id}/role`), disable/enable,
password reset, and delete with confirmation. Demoting the last `Owner`
SHALL be rejected with an inline error. Delete SHALL require confirmation.

#### Scenario: Changing a role

- **WHEN** an `Owner` changes a user's role
- **THEN** the row re-renders with the new role; a last-owner demotion is
  rejected inline.

#### Scenario: Resetting a password

- **WHEN** an `Owner` resets a user's password
- **THEN** the user's password is changed; the value is not displayed and a
  success note is shown.

#### Scenario: Confirmed delete

- **WHEN** an `Owner` confirms deletion of a user
- **THEN** the user is deleted and the list re-renders without them.
