# Spec delta: software-center

## ADDED Requirements

### Requirement: Server-owned config manifest

The Software Center SHALL expose a server-owned (fixed, code-static)
config manifest that maps panel-managed System components to their key
config file paths relative to the service config root. The manifest SHALL
be hard-coded server code and SHALL NOT be replaceable by catalog data,
environment variables, or any client-supplied value. A request that names
a component with no manifest entry SHALL be rejected before any file
access. Only the component's key file (the first entry of the manifest's
file list) SHALL be editable in this phase.

Supported components:

- `fail2ban` → `/etc/fail2ban/jail.local` (validatable)
- `nginx` → `/etc/nginx/nginx.conf` (validatable)
- `php-8.0`…`php-8.3` → `/etc/php/<version>/fpm/php.ini` (validatable)
- `mysql` → `/etc/mysql/mysql.conf.d/mysqld.cnf` (validatable)
- `mariadb` → `/etc/mysql/mariadb.conf.d/50-server.cnf` (validatable)
- `redis` → `/etc/redis/redis.conf` (validatable)

Paths SHALL resolve against a config root that defaults to `/` and is
overridable at construction time for tests.

#### Scenario: Owner opens fail2ban configuration

- **WHEN** an Owner opens the configuration surface for the `fail2ban`
  component
- **THEN** the panel renders the key config file
  `/etc/fail2ban/jail.local` (resolved against the config root) with the
  current content in the editor.

#### Scenario: Attempt to write an unlisted path

- **WHEN** a request attempts to read or write a path not named by the
  manifest for the requested component (or names a component absent from
  the manifest)
- **THEN** the request is rejected before any file access with an
  `Unsupported` error, and no file is read or written.

### Requirement: Read configuration is owner-only and bounded

Reading a managed component's config file SHALL require the Owner role
and that the component is panel-managed. The read response SHALL return
the absolute path, whether the file exists, and — when it exists — the
file content bounded to a hard ceiling (128 KiB). A missing file is NOT
an error: the response SHALL carry `exists: false` with no content.

#### Scenario: Config file exists

- **WHEN** an Owner reads the config of a managed component whose key
  file exists
- **THEN** the response contains the absolute path, `exists: true`, and
  the file content truncated nowhere: content is returned as-is when
  under the 128 KiB ceiling and the request is rejected if the file
  exceeds the ceiling.

#### Scenario: Config file does not exist yet

- **WHEN** an Owner reads the config of a managed component whose key
  file has not been created
- **THEN** the response contains the absolute path and `exists: false`
  with no content, and the editor renders an empty (or "create new")
  state rather than an error.

#### Scenario: Non-owner reads configuration

- **WHEN** a User or Admin requests a component config read through any
  surface
- **THEN** the request is rejected with `Forbidden` before any file
  access.

### Requirement: Save configuration is atomic and validated

Saving a managed component's config file SHALL require the Owner role
and that the component is panel-managed. The save SHALL write the file
atomically (write to a sibling `.new` file, then rename over the
target). When the manifest marks the component as validatable and a
validation pass fails after the write, the previous file content SHALL
be restored (or the file removed if it did not exist) and the save SHALL
be reported as failed with the validation error. Content SHALL be bounded
to the same 128 KiB ceiling used for reads. A successful save SHALL
return the absolute path written.

#### Scenario: Valid save is readable back

- **WHEN** an Owner saves new content for a managed component's config
  file
- **THEN** the file is written atomically at the manifest path and a
  subsequent read returns the saved content verbatim.

#### Scenario: Validation failure restores previous content

- **WHEN** an Owner saves content for a validatable component and the
  post-write validation pass fails
- **THEN** the previous file content is restored (or the file removed if
  it was absent), the save is reported as failed with the bounded
  validation error, and a subsequent read returns the previous content.

### Requirement: Schema and stability

The behavior described by the config-manifest, read, and save
requirements SHALL be preserved in a follow-up change that moves the
manifest into the signed catalog or adds automatic service reload after
a successful save. New manifest fields (`allow_reload`, additional edit
files) may extend but SHALL NOT alter the existing read/save contract.

#### Scenario: Follow-up moves the manifest

- **WHEN** a later change retains the manifest content but sources it
  from the signed catalog instead of server code
- **THEN** the read/save contract above is unchanged: owner-only, bounded,
  atomic, validated writes to the same key file paths.