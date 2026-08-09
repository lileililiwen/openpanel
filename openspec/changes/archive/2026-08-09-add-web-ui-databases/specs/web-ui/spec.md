## ADDED Requirements

### Requirement: Databases List

The `web-ui` SHALL render `GET /databases` inside the shell: a table with one
row per database visible to the caller (name, owner, character set) and an
"add database" action where permitted. Rows SHALL expose password and delete
actions only when the caller may perform them.

#### Scenario: Listing databases

- **WHEN** an authenticated user requests `/databases`
- **THEN** the server renders the databases table with each visible
  database's name, owner, and character set.

#### Scenario: Empty database list

- **WHEN** the caller has no databases
- **THEN** the page renders an empty state with an "add database" button (if
  permitted).

### Requirement: Create Database

The UI SHALL provide `GET /databases/new` (form) and `POST /databases`
(create). Validation and provisioning errors SHALL render inline; success
SHALL render the updated list.

#### Scenario: Creating a database

- **WHEN** an authorized caller submits a valid create form
- **THEN** the database is provisioned (with its encrypted credential) and
  the list re-renders with the new row.

### Requirement: One-Time Password Panel

The UI SHALL provide a password panel on the database detail page. Rotating
(`POST /databases/{id}/password`) or revealing (`POST /databases/{id}/reveal`)
SHALL render the plaintext credential exactly once in a transient panel; the
list and persisted HTML SHALL NOT contain plaintext credentials.

#### Scenario: Changing a password

- **WHEN** an authorized caller rotates a database password
- **THEN** the new plaintext credential is shown once in the password panel
  and the panel notes when it was rotated.

### Requirement: Delete Database

The UI SHALL render a confirmation before `DELETE /databases/{id}`. Confirmed
deletes remove the database and re-render the list.

#### Scenario: Deleting a database

- **WHEN** an authorized caller confirms deletion
- **THEN** the database (and its MySQL user/grants) is removed and the list
  re-renders without it.
