# software-center delta

## ADDED Requirements

### Requirement: Version-Targeted Installs

The Software Center SHALL accept an optional `version` on preview and
install operations. A missing version resolves to the entry's
`latest_version`; an unknown version (not in the entry's `versions`)
is rejected with a stable error.

#### Scenario: Explicit version install

- **WHEN** an authorized operator previews install of an entry with
  `version` set to a listed `VersionSpec`
- **THEN** the preview resolves packages from that version's spec and
  the install records a component row keyed by (component id, version).

#### Scenario: Unknown version rejected

- **WHEN** an operator requests install with a version absent from the
  entry's `versions`
- **THEN** the service returns `SoftwareCenterError::UnknownVersion`
  and no component row is written.

#### Scenario: Default version unchanged behaviour

- **WHEN** an operator installs without specifying a version
- **THEN** the latest version is installed, matching pre-change
  behaviour.

### Requirement: Co-Installable Versions

The software center SHALL allow multiple installed versions of the same
component to coexist; inventory and update badges are computed per
(component id, version) row against the catalog `latest_version`.

#### Scenario: Two versions installed

- **WHEN** versions `1.0` and `2.0` of a component are both installed
- **THEN** inventory lists both rows and the component is reported
  installed.

#### Scenario: Update badge per version

- **WHEN** an installed version differs from the catalog
  `latest_version` while another installed version equals it
- **THEN** the update-needed set includes only the outdated version row.

#### Scenario: Uninstall one version keeps the other

- **WHEN** an operator uninstalls a specific version while another
  version of the same component remains installed
- **THEN** only that (id, version) row is removed and audit records the
  version.
