# Add themeable UI and white-label

## Why

`refine-web-ui-with-audit-accessibility-theming` registered the
theme-token contract and the `tokens.css` source of truth. To
turn the panel into a reseller-deployable product, tokens must
be overridable per reseller account, with a logo, color palette,
typography, and even a custom panel domain. cPanel's
branding add-on and Plesk's white-label let resellers ship a
panel under their own identity; OpenPanel lacks both the
override mechanism and the panel-domain serving.

## What Changes

- New bounded context `themeable-ui` carrying the
  `ThemeOverride` aggregate and a per-domain content negotiation
  hook.
- New endpoints: `GET/PUT /admin/branding`,
  `PUT /admin/branding/logo`,
  `PUT /admin/branding/palette`.
- Hosting plan couples: a plan MAY declare a maximum branding
  scope (`System` / `Reseller` / `Disabled`).
- A per-account panel domain may be served on a dedicated
  virtual host; the panel routes the request by `Host:` header.

## Capabilities

### New Capabilities

- `themeable-ui`: per-account theme override and panel-domain
  routing.

## Impact

- Domain: `ThemeOverride`, `Palette`, `PanelDomain`.
- App: `BrandingService`, host header router.
- API/CLI/web: `/admin/branding/*`; CLI
  `openpanel branding {get,set,logo,palette}`; web
  `/admin/branding`.
- Coupling: depends on `refine-web-ui-with-audit-accessibility-theming`
  for the tokens.
