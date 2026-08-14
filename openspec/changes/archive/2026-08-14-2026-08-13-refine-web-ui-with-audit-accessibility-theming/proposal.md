# Refine web-ui with audit log, accessibility, theming hooks, and partial-i18n

## Why

`openspec/specs/web-ui/spec.md` defines the shell, navigation,
session, and CSRF rules. The follow-on `add-log-viewer`,
`refine-quality-with-i18n-and-theme-policy`, `add-themeable-ui-and-white-label`,
and `add-i18n-and-localization` changes need formal hooks in
the existing web-ui spec (router mounts, route names, theme-token
contract, accessibility floor, audit log route). This refinement
adds the formal hooks without implementing them; behaviour lands
in the follow-on changes.

## What Changes

- New route group `/audit` is registered as a known shell route
  with a placeholder handler returning 501.
- New accessibility floor requirement (per
  `refine-quality-with-i18n-and-theme-policy`): WCAG 2.1 AA conformance for
  every shipped route.
- New theme-token contract: every `maud` template MUST source
  colors, spacing, and typography from CSS custom properties
  declared in a single `tokens.css`.
- String-table contract: every user-visible string MUST come
  from a typed `T(key)` lookup; no literal user-visible strings
  in templates.

## Capabilities

### Modified Capabilities

- `web-ui`: registered audit route, accessibility floor,
  theme-token contract, string-table contract.

## Impact

- App: `web-ui` registry declares a `/audit` route group;
  ship a `tokens.css` and a typed `t.rs` stub.
- API/web: no behaviour change; placeholder 501 route returns
  `not_implemented` audit-safe body.
