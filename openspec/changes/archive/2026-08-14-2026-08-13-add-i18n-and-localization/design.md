# Add i18n and localization — Design

## Catalog model

```rust
pub struct Catalog {
    pub locale: Locale,                    // BCP-47 e.g. "en-US"
    pub messages: BTreeMap<MessageKey, Message>,
    pub plurals: BTreeMap<MessageKey, PluralForms>,
    pub metadata: CatalogMeta,
}

pub struct Message {
    pub value: String,
    pub placeholders: Vec<String>,
    pub context: Option<String>,
}
```

## Resolution

```
resolve(key, args) -> string:
  - take user.preferred_locale (cookie > header > default)
  - lookup in catalog; fallback chain: locale → language
    (e.g. "en-US" → "en") → default ("en-US")
  - apply placeholders via FormatService (ICU MessageFormat
    subset)
```

## Number / Date / Time

`Formatter::format_number`, `format_currency`,
`format_date`, `format_time` take a typed value and locale;
results respect locale rules (e.g. `,` decimal in `de-DE`,
right-to-left handling in `ar-SA`).

## Translation workflow

```
POST /locales/{locale}/translations   body: { key, value, context? }
```

Owners and translators may submit translations; the panel
signs each translation with the same publisher key as
catalog entries. A signed translation set is loaded on
restart.

## Endpoints

```
GET    /api/v1/locales
GET    /api/v1/locales/{locale}
POST   /api/v1/locales/{locale}/translations
GET    /api/v1/locales/{locale}/export              → JSON download
```

## CLI

```
openpanel i18n list
openpanel i18n show <locale>
openpanel i18n set  --user <u> --locale <l>
openpanel i18n export <locale> > messages.json
```

## Tests

```
1.1  Unit: locale fallback chain; plural form selection;
      format_number / format_date locale rules.
1.2  Property: every Message.key exists in en-US; plural
      forms match CLDR `one`/`other` for each locale.
1.3  Service tests with mock catalog and user prefs.
1.4  Integration: live locale switch on the web shell.
1.5  CLI E2E.
1.6  Web: locale switcher (CSRF), translated strings visible.
```
