## Purpose

Turns the panel into a reseller-deployable product by adding
per-account theme overrides (logo, palette, typography) and
optional custom panel-domain routing. The override is
content-negotiated by `Host` and rendered against the same
token contract; WCAG AA contrast is enforced server-side.

# themeable-ui Specification

## Requirements

### Requirement: Per-Account Theme Override

The system SHALL let authorised Owners set a `ThemeOverride`
for their own account. The override carries `brand_name`, a
`logo_path`, a `palette`, a `typography`, and an optional
`panel_domain`. Overrides apply only to the `Host` headers
that match the configured `panel_domain.fqdn`; requests on
the panel's canonical host use the system default theme.

#### Scenario: Override applies to panel domain

- **WHEN** an Owner has a `ThemeOverride` whose `panel_domain.fqdn = "panel.acme.com"` and a request arrives with `Host: panel.acme.com`
- **THEN** the rendered shell uses the override's brand name, logo, palette, and typography.

#### Scenario: System host unaffected

- **WHEN** the same Owner makes a request to the canonical panel host
- **THEN** the system default theme is rendered; the override is not applied.

### Requirement: Palette Validation

The system SHALL validate palette updates server-side: the
contrast ratio between `color_fg` and `color_bg` MUST be at
least `contrast_min` (default 4.5, configurable per account,
enforced at or above 4.5). Each palette update SHALL be
rejected with `ThemeError::InsufficientContrast{pair, ratio}`
if any pair's ratio is below the floor.

#### Scenario: Insufficient contrast rejected

- **WHEN** an Owner submits `color_fg = #aaa` and `color_bg = #bbb`
- **THEN** the request is rejected with `InsufficientContrast{pair="fg:bg", ratio=2.3}` and no change.

#### Scenario: Sufficient contrast accepted

- **WHEN** the Owner submits `color_fg = #fff` and `color_bg = #000`
- **THEN** the request is accepted and the override updates.

### Requirement: Logo Upload

Logo uploads SHALL accept SVG (sanitised against `<script>`,
`on*` attributes, external hrefs), PNG (≤ 256 KiB), or JPEG
(≤ 256 KiB). The panel refuses unsupported MIME types and
oversized files; stored logos are served from `/srv/branding/
<owner>/logo.<ext>` and rendered by the shell template.

#### Scenario: SVG sanitisation

- **WHEN** an Owner uploads an SVG containing `<script>alert(1)</script>`
- **THEN** the panel refuses the upload with `ThemeError::SvgContainsForbidden` and the audit logs the file name only.

#### Scenario: Oversized PNG

- **WHEN** the file exceeds 256 KiB
- **THEN** the upload is rejected with `ThemeError::FileTooLarge`.

### Requirement: Panel Domain Routing

A `panel_domain.fqdn` SHALL be served by the panel under a
dedicated TLS virtual host; the panel SHALL refuse to bind
without a previously-issued certificate (the `ssl` capability
is the source of truth for certificates). The routing decision
is based on the `Host` header exclusively; query path
matching is not used.

#### Scenario: fqdn served

- **WHEN** the `ssl` capability has issued a cert for `panel.acme.com` and the fqdn is set
- **THEN** nginx serves the panel under that name and the override theme is rendered.

#### Scenario: Missing certificate

- **WHEN** the Owner submits an fqdn with no matching certificate in the `ssl` capability
- **THEN** the request is rejected with `ThemeError::NoMatchingCertificate`.

### Requirement: Hosting-Plan BrandingScope

A hosting plan MAY set `BrandingScope=Reseller` to grant the
user branding capability; the API SHALL refuse any attempt to set
`BrandingScope=System`. Users without the scope cannot read or
write branding endpoints.

#### Scenario: System scope is refused

- **WHEN** an API request assigns `BrandingScope=System` to a plan
- **THEN** the API rejects the change and the previous scope is
        preserved.

#### Scenario: Plan grants scope

- **WHEN** an Owner is on a plan with `BrandingScope=Reseller`
- **THEN** the Owner can PUT a `ThemeOverride` for their own account.

#### Scenario: Plan denies scope

- **WHEN** a User is on a plan with `BrandingScope=Disabled`
- **THEN** the API returns 403 for any branding endpoint.
