# Refine DNS with zone templates

## Why

`openspec/specs/dns/spec.md` describes lifecycle of provider accounts,
zones, and records but does not pin a **default record set** applied
when a new zone is enabled. Baota and cPanel both apply a safe
default template (apex A/AAAA, MX, SPF, DMARC, CAA, DKIM) the moment
a domain is enabled, so an operator never ships a domain without
SPF / DMARC and so Let's Encrypt issuance works on first attempt.
OpenPanel currently lets an enabled zone be empty, which is unsafe by
default and unfriendly to new operators. This change adds a typed
default-template mechanism with explicit per-record override.

## What Changes

- New value object `ZoneTemplate` and `TemplateRecord` describing
  the default record set applied to every newly-enabled zone.
- `dns::Zone::enable(owner)` applies the template atomically before
  persisting `Zone.status = Active`; records that fail provider
  validation are stored as `Pending` with a redacted diagnostic.
- New endpoint `POST /api/v1/dns/zones/{id}/apply-template` to
  re-apply or preview a template; CLI `openpanel dns apply-template`;
  web action on the zone detail page.
- New bounded sub-domain `dns::TemplateRegistry` lists the built-in
  templates plus optional per-owner overrides (`strict`, `relaxed`,
  `parked`).

## Capabilities

### Modified Capabilities

- `dns`: zone enablement MUST apply a default template; per-zone
  template overrides; template apply/preview operations.

## Impact

- Domain: `ZoneTemplate`, `TemplateRecord` (kind, name, ttl, value,
  policy `Required` / `Optional`).
- App: `TemplateApplier` in `openpanel-app/dns/`, integrating with
  the existing `ProviderAccountRegistry`.
- API/CLI/web: `POST /dns/zones/{id}/apply-template`,
  `openpanel dns apply-template`, web action button.
- Built-in templates live in
  `crates/openpanel-domain/src/dns/templates.json` and are loaded
  as compiled-in defaults; new templates can be added by editing
  the JSON plus a manifest signature check.
