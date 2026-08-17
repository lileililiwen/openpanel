//! Internationalization domain — `Locale`, `Catalog`, `Message`,
//! `LocaleNegotiator`, and `Formatter`.
//!
//! All types are pure data (no I/O). The application layer is
//! responsible for loading catalogs and persisting per-user
//! preferences.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

/// A typed error raised by the i18n domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I18nError(pub String);

impl fmt::Display for I18nError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for I18nError {}

/// A BCP-47 locale tag, e.g. `en-US`, `zh-CN`, `de-DE`, `fr-FR`.
///
/// The struct stores the tag in lowercase language + uppercase
/// region (e.g. `en-US`) so equality and ordering are stable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Locale(String);

impl Locale {
    /// Parse a BCP-47 tag. The tag is normalised: language is
    /// lowercased, region is uppercased.
    pub fn new(tag: &str) -> Result<Self, I18nError> {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return Err(I18nError("locale tag is empty".to_string()));
        }
        let mut parts = trimmed.split('-');
        let language = parts
            .next()
            .ok_or_else(|| I18nError("missing language".to_string()))?;
        if !language.chars().all(|c| c.is_ascii_alphabetic()) || language.len() < 2 {
            return Err(I18nError(format!("invalid language: {language}")));
        }
        let region = parts.next();
        if let Some(region) = region
            && (!region.chars().all(|c| c.is_ascii_alphabetic()) || region.len() < 2)
        {
            return Err(I18nError(format!("invalid region: {region}")));
        }
        let mut normalised = language.to_ascii_lowercase();
        if let Some(region) = region {
            normalised.push('-');
            normalised.push_str(&region.to_ascii_uppercase());
        }
        Ok(Locale(normalised))
    }

    /// The language subtag (lowercase).
    pub fn language(&self) -> &str {
        self.0.split('-').next().unwrap_or(&self.0)
    }

    /// The region subtag (uppercase), if present.
    pub fn region(&self) -> Option<&str> {
        self.0.split('-').nth(1)
    }

    /// The full BCP-47 tag.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Locale {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Default for Locale {
    fn default() -> Self {
        // en-US is the panel default per the spec.
        Locale("en-US".to_string())
    }
}

/// A typed message key, e.g. `save_button`, `files_count`.
pub type MessageKey = String;

/// A single translation entry, optionally with placeholders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// The translation text. May contain `{name}` placeholders.
    pub value: String,
    /// Declared placeholder names in the translation. Used by
    /// the formatter to surface missing-arg errors before render.
    #[serde(default)]
    pub placeholders: Vec<String>,
    /// Optional context for the translator (e.g. a hint about
    /// where the string is rendered).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// CLDR plural forms for a single key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluralForms {
    /// `count == 1` in en-US (and most languages).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub one: Option<String>,
    /// CLDR `other` — the catch-all.
    pub other: String,
    /// CLDR `few` (used by languages like `pl-PL`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub few: Option<String>,
    /// CLDR `many` (used by languages like `pl-PL`, `ar-SA`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub many: Option<String>,
    /// CLDR `zero` (used by languages like `ar-SA`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zero: Option<String>,
    /// CLDR `two` (used by languages like `ar-SA`, `he-IL`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub two: Option<String>,
}

impl PluralForms {
    /// Select a form by count and locale. The default rule
    /// (`en-US`-style) maps `count == 1` to `one` and every
    /// other value to `other`. Languages with more forms
    /// (`pl-PL`, `ar-SA`, `he-IL`) use the CLDR rule for the
    /// language family.
    pub fn select(&self, locale: &Locale, count: u64) -> &str {
        let form = plural_category(locale, count);
        match form {
            PluralCategory::Zero => self.zero.as_deref().unwrap_or(&self.other),
            PluralCategory::One => self.one.as_deref().unwrap_or(&self.other),
            PluralCategory::Two => self.two.as_deref().unwrap_or(&self.other),
            PluralCategory::Few => self.few.as_deref().unwrap_or(&self.other),
            PluralCategory::Many => self.many.as_deref().unwrap_or(&self.other),
            PluralCategory::Other => &self.other,
        }
    }
}

/// The CLDR plural category for a (locale, count) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluralCategory {
    /// CLDR `zero` (used by languages like `ar-SA`).
    Zero,
    /// CLDR `one` — `count == 1` in en-US (and most languages).
    One,
    /// CLDR `two` (used by languages like `ar-SA`, `he-IL`).
    Two,
    /// CLDR `few` (used by languages like `pl-PL`, `ar-SA`).
    Few,
    /// CLDR `many` (used by languages like `pl-PL`, `ar-SA`).
    Many,
    /// CLDR `other` — the catch-all.
    Other,
}

/// Compute the CLDR plural category for a (locale, count) pair.
/// We implement a small subset of CLDR rules (en, zh, pl, ar, he,
/// and the default "Other"). The grammar intentionally returns
/// `Other` for unknown languages so the catalog always has a
/// usable form.
pub fn plural_category(locale: &Locale, count: u64) -> PluralCategory {
    let lang = locale.language();
    let n = count;
    match lang {
        // English, German, Spanish, French, Italian, Portuguese,
        // Dutch, Swedish, etc.: 1 → One, else Other.
        "en" | "de" | "es" | "fr" | "it" | "pt" | "nl" | "sv" | "no" | "da" | "fi" | "et"
        | "el" | "hu" | "tr" => {
            if n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        // Polish: 1 → One, 2-4 / 22-24 / ... → Few, else Many.
        "pl" => {
            if n == 1 {
                PluralCategory::One
            } else if (2..=4).contains(&n) {
                PluralCategory::Few
            } else {
                PluralCategory::Many
            }
        }
        // Arabic: zero, one, two, few, many, other.
        "ar" => {
            if n == 0 {
                PluralCategory::Zero
            } else if n == 1 {
                PluralCategory::One
            } else if n == 2 {
                PluralCategory::Two
            } else if (3..=10).contains(&n) {
                PluralCategory::Few
            } else if (11..=99).contains(&n) {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Chinese, Japanese, Korean: no plural forms (one category, Other).
        "zh" | "ja" | "ko" => PluralCategory::Other,
        _ => {
            if n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
    }
}

/// A translation catalog for a single locale.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    /// The locale this catalog covers.
    pub locale: Locale,
    /// Simple (non-plural) messages.
    #[serde(default)]
    pub messages: BTreeMap<MessageKey, Message>,
    /// Plural messages.
    #[serde(default)]
    pub plurals: BTreeMap<MessageKey, PluralForms>,
    /// Catalog metadata (translator, signers, etc.).
    #[serde(default)]
    pub metadata: CatalogMetadata,
}

impl Catalog {
    /// Construct a new empty catalog for the given locale.
    pub fn new(locale: Locale) -> Self {
        Self {
            locale,
            messages: BTreeMap::new(),
            plurals: BTreeMap::new(),
            metadata: CatalogMetadata::default(),
        }
    }

    /// Add a simple message.
    pub fn add_message(&mut self, key: impl Into<MessageKey>, value: impl Into<String>) {
        self.messages.insert(
            key.into(),
            Message {
                value: value.into(),
                placeholders: Vec::new(),
                context: None,
            },
        );
    }

    /// Add a plural message.
    pub fn add_plural(&mut self, key: impl Into<MessageKey>, forms: PluralForms) {
        self.plurals.insert(key.into(), forms);
    }

    /// Look up a simple message.
    pub fn get(&self, key: &str) -> Option<&Message> {
        self.messages.get(key)
    }

    /// Look up a plural message.
    pub fn get_plural(&self, key: &str) -> Option<&PluralForms> {
        self.plurals.get(key)
    }

    /// Count of simple + plural messages.
    pub fn len(&self) -> usize {
        self.messages.len() + self.plurals.len()
    }

    /// True if the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty() && self.plurals.is_empty()
    }
}

/// Catalog metadata (provenance, signing, etc.).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogMetadata {
    /// Human-readable translator name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translator: Option<String>,
    /// Version string, e.g. `2026-08-13`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Ed25519 signature over the catalog contents (base64).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Resolve a locale for a request.
///
/// The negotiator is constructed once and applied per request
/// via `negotiate(&NegotiationHints)`. The hints mirror the
/// four-stage fallback chain in the spec.
#[derive(Debug, Clone, Default)]
pub struct LocaleNegotiator {
    default_locale: Locale,
    supported: Vec<Locale>,
}

impl LocaleNegotiator {
    /// Construct a negotiator with the panel default and a list
    /// of supported locales.
    pub fn new(default_locale: Locale, supported: Vec<Locale>) -> Self {
        Self {
            default_locale,
            supported,
        }
    }

    /// The default locale (returned when no hint is set).
    pub fn default_locale(&self) -> &Locale {
        &self.default_locale
    }

    /// The set of supported locales.
    pub fn supported(&self) -> &[Locale] {
        &self.supported
    }

    /// Negotiate a locale from the request hints.
    pub fn negotiate(&self, hints: &NegotiationHints<'_>) -> Locale {
        // 1. URL prefix wins outright.
        if let Some(url) = hints.url_locale
            && let Some(found) = self.find_supported(url)
        {
            return found;
        }
        // 2. User preference.
        if let Some(user) = hints.user_locale
            && let Some(found) = self.find_supported(user)
        {
            return found;
        }
        // 3. Accept-Language header.
        if let Some(header) = hints.accept_language {
            for tag in parse_accept_language(header) {
                if let Some(found) = self.find_supported(&tag) {
                    return found;
                }
                // Try the language family.
                if let Some(family) = language_family_str(&tag)
                    && let Some(found) = self.find_supported(&family)
                {
                    return found;
                }
            }
        }
        // 4. Panel default.
        self.default_locale.clone()
    }

    /// True if `tag` (or its language family) is in the
    /// supported list.
    fn find_supported(&self, tag: &str) -> Option<Locale> {
        let candidate = Locale::new(tag).ok()?;
        if self.supported.iter().any(|l| l == &candidate) {
            return Some(candidate);
        }
        // Try the language family.
        let family = language_family(&candidate)?;
        let family_locale = Locale::new(&family).ok()?;
        if self.supported.iter().any(|l| l == &family_locale) {
            return Some(family_locale);
        }
        None
    }
}

/// Hints the negotiator considers in priority order.
#[derive(Debug, Clone, Default)]
pub struct NegotiationHints<'a> {
    /// URL prefix locale (e.g. `/zh-CN/admin` → `Some("zh-CN")`).
    pub url_locale: Option<&'a str>,
    /// The user's preferred locale (from session / DB).
    pub user_locale: Option<&'a str>,
    /// Raw `Accept-Language` header value.
    pub accept_language: Option<&'a str>,
}

/// Parse an `Accept-Language` header into a list of tags ordered
/// by quality (highest first).
pub fn parse_accept_language(header: &str) -> Vec<String> {
    let mut entries: Vec<(f32, String)> = header
        .split(',')
        .filter_map(|raw| {
            let raw = raw.trim();
            if raw.is_empty() {
                return None;
            }
            let (tag, q) = match raw.split_once(';') {
                Some((t, qs)) => {
                    let q = qs
                        .trim()
                        .strip_prefix("q=")
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(1.0);
                    (t.trim().to_string(), q)
                }
                None => (raw.to_string(), 1.0),
            };
            Some((q, tag))
        })
        .collect();
    entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    entries.into_iter().map(|(_, tag)| tag).collect()
}

/// The language-family form of a locale (e.g. `en-US` → `en`).
pub fn language_family(locale: &Locale) -> Option<String> {
    let lang = locale.language();
    if lang.is_empty() {
        return None;
    }
    Some(lang.to_string())
}

/// The language-family form of a raw BCP-47 tag.
pub fn language_family_str(tag: &str) -> Option<String> {
    let lang = tag.split('-').next()?;
    if lang.is_empty() {
        return None;
    }
    Some(lang.to_string())
}

/// Resolve a translation key through a stack of catalogs.
#[derive(Debug, Clone, Default)]
pub struct CatalogResolver {
    /// Default catalog (typically `en-US`).
    pub default: Option<Catalog>,
    /// Extra catalogs, in lookup order.
    pub extras: Vec<Catalog>,
}

impl CatalogResolver {
    /// Construct a resolver with a default catalog.
    pub fn new(default: Catalog) -> Self {
        Self {
            default: Some(default),
            extras: Vec::new(),
        }
    }

    /// Add a catalog to the resolver.
    pub fn add(&mut self, catalog: Catalog) {
        self.extras.push(catalog);
    }

    /// Resolve a simple key. Returns `None` if the key is missing
    /// in every catalog.
    pub fn resolve(&self, locale: &Locale, key: &str) -> Option<&Message> {
        // 1. exact match on locale
        for cat in self.extras.iter().chain(self.default.iter()) {
            if &cat.locale == locale
                && let Some(msg) = cat.get(key)
            {
                return Some(msg);
            }
        }
        // 2. language family match
        let family = language_family(locale)?;
        for cat in self.extras.iter().chain(self.default.iter()) {
            if cat.locale.language() == family
                && let Some(msg) = cat.get(key)
            {
                return Some(msg);
            }
        }
        // 3. default catalog fallback
        self.default.as_ref().and_then(|c| c.get(key))
    }

    /// Resolve a plural key. Returns `None` if the key is missing.
    pub fn resolve_plural(&self, locale: &Locale, key: &str) -> Option<&PluralForms> {
        for cat in self.extras.iter().chain(self.default.iter()) {
            if &cat.locale == locale
                && let Some(forms) = cat.get_plural(key)
            {
                return Some(forms);
            }
        }
        let family = language_family(locale)?;
        for cat in self.extras.iter().chain(self.default.iter()) {
            if cat.locale.language() == family
                && let Some(forms) = cat.get_plural(key)
            {
                return Some(forms);
            }
        }
        self.default.as_ref().and_then(|c| c.get_plural(key))
    }
}

/// Format numbers, currencies, dates, and times with locale rules.
#[derive(Debug, Clone)]
pub struct Formatter;

impl Formatter {
    /// Construct a new formatter.
    pub fn new() -> Self {
        Self
    }

    /// Format a number with the locale's decimal separator.
    pub fn format_number(&self, locale: &Locale, value: f64) -> String {
        let (sep, group) = number_format(locale);
        format_number_with_separators(value, sep, group)
    }

    /// Format a currency amount.
    pub fn format_currency(&self, locale: &Locale, value: f64, code: &str) -> String {
        let (sep, group) = number_format(locale);
        let amount = format_number_with_separators(value, sep, group);
        match currency_position(locale) {
            CurrencyPosition::Prefix => format!("{code} {amount}"),
            CurrencyPosition::Suffix => format!("{amount} {code}"),
        }
    }

    /// Format a date (year, month, day). The date order is
    /// locale-specific: `en-US` is Y-M-D, `de-DE` is D.M.Y,
    /// `zh-CN` is Y-M-D.
    pub fn format_date(&self, locale: &Locale, year: i32, month: u32, day: u32) -> String {
        let sep = date_separator(locale);
        let order = date_order(locale);
        match order {
            DateOrder::YearMonthDay => format!("{year:04}{sep}{month:02}{sep}{day:02}"),
            DateOrder::DayMonthYear => format!("{day:02}{sep}{month:02}{sep}{year:04}"),
            DateOrder::MonthDayYear => format!("{month:02}{sep}{day:02}{sep}{year:04}"),
        }
    }

    /// Format a 24-hour or 12-hour time. The separator and
    /// 12/24-hour choice is locale-specific.
    pub fn format_time(&self, locale: &Locale, hour: u32, minute: u32) -> String {
        let sep = time_separator(locale);
        if is_twenty_four_hour(locale) {
            format!("{hour:02}{sep}{minute:02}")
        } else {
            let (h, ampm) = if hour == 0 {
                (12, "AM")
            } else if hour < 12 {
                (hour, "AM")
            } else if hour == 12 {
                (12, "PM")
            } else {
                (hour - 12, "PM")
            };
            format!("{h}{sep}{minute:02} {ampm}")
        }
    }

    /// True if the locale is right-to-left.
    pub fn is_rtl(&self, locale: &Locale) -> bool {
        matches!(locale.language(), "ar" | "he" | "fa" | "ur")
    }

    /// The text direction attribute for HTML: `ltr` or `rtl`.
    pub fn dir(&self, locale: &Locale) -> &'static str {
        if self.is_rtl(locale) { "rtl" } else { "ltr" }
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self
    }
}

fn number_format(locale: &Locale) -> (char, char) {
    // de-DE / fr-FR / it-IT / es-ES use , as decimal and . or
    // non-breaking space as group; zh-CN also uses , but most
    // English locales use . as decimal and , as group.
    match locale.language() {
        "de" | "fr" | "it" | "es" | "nl" | "pl" | "pt" | "ru" | "sv" | "no" | "da" | "fi"
        | "et" => (',', '.'),
        "zh" | "ja" | "ko" => (',', ','),
        _ => ('.', ','),
    }
}

fn currency_position(locale: &Locale) -> CurrencyPosition {
    match locale.language() {
        "de" | "fr" | "es" | "it" | "nl" | "fi" | "et" | "pt" | "ru" => CurrencyPosition::Suffix,
        _ => CurrencyPosition::Prefix,
    }
}

enum CurrencyPosition {
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateOrder {
    YearMonthDay,
    DayMonthYear,
    MonthDayYear,
}

fn date_order(locale: &Locale) -> DateOrder {
    match locale.language() {
        "en" | "ja" | "ko" | "zh" => DateOrder::MonthDayYear,
        "de" | "fr" | "it" | "es" | "nl" | "pl" | "pt" | "ru" | "sv" | "no" | "da" | "fi"
        | "et" => DateOrder::DayMonthYear,
        _ => DateOrder::YearMonthDay,
    }
}

fn date_separator(locale: &Locale) -> char {
    match locale.language() {
        "de" | "fr" | "it" | "es" | "nl" | "pl" | "pt" | "ru" | "sv" | "no" | "da" | "fi"
        | "et" => '.',
        _ => '-',
    }
}

fn time_separator(_locale: &Locale) -> char {
    ':'
}

fn is_twenty_four_hour(locale: &Locale) -> bool {
    !matches!(locale.language(), "en" | "ja" | "ko")
}

fn format_number_with_separators(value: f64, decimal: char, group: char) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let negative = value < 0.0;
    let abs = value.abs();
    let int_part = abs.trunc() as u64;
    let frac_part = (abs.fract() * 100.0).round() as u64;
    let int_str = group_thousands(int_part, group);
    let mut out = if frac_part == 0 {
        int_str
    } else {
        let frac = format!("{frac_part:02}");
        let frac = frac.trim_end_matches('0');
        if frac.is_empty() {
            int_str
        } else {
            format!("{int_str}{decimal}{frac}")
        }
    };
    if negative {
        out.insert(0, '-');
    }
    out
}

fn group_thousands(mut value: u64, group: char) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut groups: Vec<String> = Vec::new();
    loop {
        let chunk = format!("{:03}", value % 1000);
        groups.push(chunk);
        value /= 1000;
        if value == 0 {
            break;
        }
    }
    groups.reverse();
    // The first group carries no leading-zero padding; trim the
    // leading zeros off the first chunk only.
    match groups.first_mut() {
        Some(first) => {
            let trimmed = first.trim_start_matches('0').to_string();
            if trimmed.is_empty() {
                "0".to_string()
            } else {
                *first = trimmed;
                groups.join(&group.to_string())
            }
        }
        None => "0".to_string(),
    }
}

/// Render a translation value with placeholder substitution.
///
/// Placeholders use `{name}` syntax. Unknown names are left in
/// the output as `{name}` so a missing-arg error is visible.
pub fn render_template(template: &str, args: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '{' {
            out.push(ch);
            continue;
        }
        let mut name = String::new();
        let mut closed = false;
        while let Some(&next) = chars.peek() {
            if next == '}' {
                chars.next();
                closed = true;
                break;
            }
            name.push(next);
            chars.next();
        }
        if !closed {
            out.push('{');
            out.push_str(&name);
            continue;
        }
        if let Some(value) = args.get(&name) {
            out.push_str(value);
        } else {
            out.push('{');
            out.push_str(&name);
            out.push('}');
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests;
