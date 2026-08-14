# i18n Specification

## Purpose
TBD - created by archiving change 2026-08-13-add-i18n-and-localization. Update Purpose after archive.
## Requirements
### Requirement: Locale Negotiation

`LocaleNegotiator` SHALL resolve a locale for every request in
this order: explicit URL locale prefix; `User.preferred_locale`;
`Accept-Language` header; panel default (`en-US`). The
negotiator MUST be safe to call on every authenticated
request and MUST cache results per session for the request
lifetime.

#### Scenario: URL prefix

- **WHEN** a request arrives at `/zh-CN/admin`
- **THEN** the negotiated locale is `zh-CN` for this request only.

#### Scenario: User preference

- **WHEN** `User.preferred_locale = "fr-FR"` and no URL prefix is set
- **THEN** the negotiated locale is `fr-FR`.

#### Scenario: Header fallback

- **WHEN** no URL prefix or user preference is set, and `Accept-Language: ja-JP, ja;q=0.9, en;q=0.5`
- **THEN** the negotiated locale is the highest-quality match the panel supports.

### Requirement: Catalog Resolution

The system SHALL ship a default English catalog and MAY load
additional catalogs from the SQLite-backed translation store.
The fallback chain SHALL be: requested locale → language
family (e.g. `fr-FR` → `fr`) → default (`en-US`). Missing
keys SHALL return the key itself with a `[MISSING]` prefix
in logs only (never in user-facing output) — user output
falls back to en-US for the same key.

#### Scenario: Localized lookup

- **WHEN** `T("save_button")` is called with negotiated locale `fr-FR` and the catalog has `save_button = "Enregistrer"`
- **THEN** the rendered text is `Enregistrer`.

#### Scenario: Missing key

- **WHEN** `T("brand_new_button")` is not in any catalog
- **THEN** the rendered text is `brand_new_button`; the
        audit `I18nMissingKey{key, locale}` is emitted with
        the locale label only.

### Requirement: Placeholder and Plural Formatting

The catalog SHALL support ICU MessageFormat-style placeholders
and plural forms. A plural form SHALL be selected by a `count`
argument; for `en-US` the rules are `one` (count == 1) and
`other`.

#### Scenario: Plural-form selection

- **WHEN** `T("files_count", count=1)` is called and the
        catalog has `"one": "1 file"`, `"other": "{count} files"`
- **THEN** the rendered text is `1 file`.

#### Scenario: Locale-specific plural rule

- **WHEN** the negotiated locale is `pl-PL` and `count = 5`
- **THEN** the rendered text uses the `many` form per CLDR
        Polish plural rules.

### Requirement: Locale-Aware Formatting

`Formatter::format_number`, `format_currency`, `format_date`,
`format_time` SHALL apply locale-specific rules: decimal
separator (`en-US=., de-DE=,`); currency position; date order;
right-to-left shape.

#### Scenario: Number formatting

- **WHEN** an Owner renders `1234.5` for locale `de-DE`
- **THEN** the rendered text is `1.234,5`.

#### Scenario: Right-to-left layout

- **WHEN** the negotiated locale is `ar-SA`
- **THEN** the rendered shell applies `dir="rtl"` and the
        locale CSS dir rules.

### Requirement: Translation Workflow

`POST /locales/{locale}/translations` SHALL accept typed
translations under a publisher key; each translation is
signed with the panel's Ed25519 publisher key. The
translation store SHALL be audited and SHALL never accept a
translation that lacks the publisher signature.

#### Scenario: Submit translation

- **WHEN** a translator posts a translation with a valid signature
- **THEN** the translation row is persisted; audit
        `TranslationSubmitted` is recorded with the locale
        and key.

#### Scenario: Signature failure

- **WHEN** a translation's signature does not verify
- **THEN** the request is rejected with `TranslationSignatureFailed`; the audit logs the locale only.

### Requirement: Locale Switcher UI

The web shell SHALL expose a locale switcher in the top bar.
Selecting a locale sets the user preference and reloads the
page in the new locale. The switcher MUST be CSRF-protected
and MUST NOT log out the user.

#### Scenario: Switch locale

- **WHEN** an Owner selects "fr-FR" in the switcher
- **THEN** `User.preferred_locale = "fr-FR"`; subsequent
        requests negotiate to `fr-FR`; the page reloads.

#### Scenario: No translation for locale

- **WHEN** the user picks a locale whose catalog is missing
        more than 30% of keys
- **THEN** the UI shows a banner with the missing-key count
        and offers an "export English fallback" action.

