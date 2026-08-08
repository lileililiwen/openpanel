## ADDED Requirements

### Requirement: Sites List

The `web-ui` SHALL render `GET /sites` inside the shell: a table with one row
per site visible to the caller (domain, status, owner, PHP version), and an
"add site" button for callers permitted to create sites. Non-owner callers
SHALL see only sites they own and SHALL NOT see create/delete actions.

#### Scenario: Listing sites

- **WHEN** an authenticated user requests `/sites`
- **THEN** the server renders the sites table with each visible site's
  domain, status, owner, and PHP version, plus row actions the caller may
  perform.

#### Scenario: Empty site list

- **WHEN** the caller has no sites
- **THEN** the page renders an empty state with an "add site" button (if
  permitted).

### Requirement: Create Site

The UI SHALL provide `GET /sites/new` (form) and `POST /sites` (create). The
form SHALL collect primary domain, aliases, owner, PHP enablement + version,
and an optional document root override. Duplicate-domain and validation errors
SHALL render as an inline alert; successful creation SHALL render the updated
site list.

#### Scenario: Creating a site

- **WHEN** an authorized caller submits a valid site form to `POST /sites`
- **THEN** the site is created (persisted, document root provisioned, nginx
  config applied) and the list re-renders with the new row.

#### Scenario: Duplicate domain

- **WHEN** a caller submits a primary domain that already exists
- **THEN** the handler returns the existing site's domain in an inline error
  alert and creates nothing.

### Requirement: Site Detail and Status

The UI SHALL render `GET /sites/{id}` with the site's domain, aliases,
document root, PHP settings, and status, plus links to its files and SSL
pages. Enable/disable SHALL be available as HTMX actions with CSRF.

#### Scenario: Enabling and disabling

- **WHEN** an authorized caller triggers enable or disable on a site
- **THEN** the site's status flips and the row re-renders with the new
  status.

### Requirement: Delete Site

The UI SHALL render a confirmation before `DELETE /sites/{id}`. Confirmed
deletes remove the site (and its nginx config) and re-render the list.

#### Scenario: Deleting a site

- **WHEN** an authorized caller confirms deletion of a site
- **THEN** the site is deleted and the list re-renders without it.
