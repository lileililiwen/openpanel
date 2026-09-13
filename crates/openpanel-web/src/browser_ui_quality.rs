//! Browser UI quality gate: route matrix, rendered accessibility /
//! responsive evaluators, typed localization resolution, and artifact
//! capture.
//!
//! Covers `browser-ui-quality`: Rendered Accessibility Gate, Responsive
//! Route Gate, Typed Localization, Reduced Motion.
//!
//! The browser harness (Playwright + axe, `tests/browser/`) proves
//! rendered behaviour in CI. This module is the deterministic,
//! I/O-free core the harness and the `check-browser-ui-quality.sh`
//! gate share: the route matrix derives from the single
//! [`crate::capability_registry`] inventory so nav, router, and site
//! workspace can never drift apart; localization reuses the domain
//! `i18n` catalog stack (`CatalogResolver`, `Formatter`,
//! `render_template`) and the app `default_catalog` instead of
//! re-implementing fallback, plural, or formatting rules.

use std::collections::BTreeMap;

use openpanel_domain::Role;
use openpanel_domain::i18n::{CatalogResolver, Formatter, Locale, render_template};

/// Capability under test: `browser-ui-quality`.
pub const BROWSER_UI_QUALITY_CAPABILITY: &str = "browser-ui-quality";

/// Viewport widths the responsive gate covers (mobile, tablet, desktop).
pub const BROWSER_UI_QUALITY_VIEWPORTS: [u32; 3] = [360, 768, 1280];

/// Concrete site id substituted for `{id}` / `{site_id}` templates.
const BROWSER_SMOKE_SITE_ID: &str = "smoke-site";
/// Concrete domain substituted for `{domain}` templates.
const BROWSER_SMOKE_DOMAIN: &str = "example.com";

/// Viewport widths for the responsive gate.
pub fn browser_quality_viewports() -> Vec<u32> {
    BROWSER_UI_QUALITY_VIEWPORTS.to_vec()
}

/// Substitute smoke values for site placeholders in a route template.
pub fn browser_concrete_path(template: &str) -> String {
    template
        .replace("{id}", BROWSER_SMOKE_SITE_ID)
        .replace("{site_id}", BROWSER_SMOKE_SITE_ID)
        .replace("{domain}", BROWSER_SMOKE_DOMAIN)
}

/// Global (sidebar) route templates in registry order.
pub fn browser_global_templates() -> Vec<&'static str> {
    crate::capability_registry::global_entries()
        .map(|e| e.route)
        .collect()
}

/// Site-scoped route templates in registry order.
pub fn browser_site_templates() -> Vec<&'static str> {
    crate::capability_registry::site_entries()
        .map(|e| e.route)
        .collect()
}

/// Every registered route as a concrete path (global + site).
pub fn browser_matrix_paths() -> Vec<String> {
    crate::capability_registry::entries()
        .iter()
        .map(|e| browser_concrete_path(e.route))
        .collect()
}

/// Concrete paths visible to `role` (defense in depth: the shell hides
/// the link AND handlers still deny direct access).
pub fn browser_paths_for_role(role: Role) -> Vec<String> {
    crate::capability_registry::entries()
        .iter()
        .filter(|e| crate::capability_registry::role_may_see(role, e))
        .map(|e| browser_concrete_path(e.route))
        .collect()
}

/// Static accessibility findings for one rendered page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowserAccessibilityReport {
    /// `<button>` / `<a>` elements with no accessible name.
    pub browser_nameless_interactives: Vec<String>,
    /// Visible `<input>` elements with no paired `<label>`.
    pub browser_unlabelled_inputs: Vec<String>,
    /// Number of `<h1>` elements on the page.
    pub browser_h1_count: usize,
    /// Whether `<html lang="…">` is present.
    pub browser_has_lang: bool,
}

/// Evaluate rendered HTML for the static accessibility contract:
/// every interactive element carries an accessible name, every visible
/// input has a paired label, exactly one `<h1>` names the page, and
/// the document declares its language.
pub fn browser_evaluate_accessibility(html: &str) -> BrowserAccessibilityReport {
    BrowserAccessibilityReport {
        browser_nameless_interactives: nameless_interactives(html),
        browser_unlabelled_inputs: unlabelled_inputs(html),
        browser_h1_count: count_tag(html, "h1"),
        browser_has_lang: html.contains("lang=\""),
    }
}

/// True when the static accessibility contract holds.
pub fn browser_accessibility_is_clean(report: &BrowserAccessibilityReport) -> bool {
    report.browser_nameless_interactives.is_empty()
        && report.browser_unlabelled_inputs.is_empty()
        && report.browser_h1_count == 1
        && report.browser_has_lang
}

/// Static responsive findings for one rendered page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowserResponsiveReport {
    /// Whether an inline fixed-unit `width:` defeats the fluid layout.
    pub browser_has_fixed_inline_width: bool,
    /// `<table>` openings with no class attribute.
    pub browser_bare_tables: usize,
    /// Whether the responsive `.layout` shell is present.
    pub browser_has_layout_shell: bool,
}

/// Evaluate rendered HTML for the responsive contract: no fixed-unit
/// inline `width:`, every `<table>` declares a class, and the fluid
/// `.layout` shell wraps the page.
pub fn browser_evaluate_responsive(html: &str) -> BrowserResponsiveReport {
    BrowserResponsiveReport {
        browser_has_fixed_inline_width: has_fixed_inline_width(html),
        browser_bare_tables: count_bare_tables(html),
        browser_has_layout_shell: html.contains("class=\"layout\""),
    }
}

/// True when the static responsive contract holds.
pub fn browser_responsive_is_clean(report: &BrowserResponsiveReport) -> bool {
    !report.browser_has_fixed_inline_width
        && report.browser_bare_tables == 0
        && report.browser_has_layout_shell
}

/// Whether the stylesheet carries the keyboard focus-ring contract.
pub fn browser_focus_ring_is_present(css: &str) -> bool {
    css.contains(":focus-visible") && css.contains("--op-color-focus-ring")
}

/// Whether the stylesheet respects `prefers-reduced-motion: reduce`
/// without removing status information (durations silenced, state
/// changes still perceivable via the layout itself).
pub fn browser_reduced_motion_is_present(css: &str) -> bool {
    css.contains("@media (prefers-reduced-motion: reduce)")
        && css.contains("animation-duration")
        && css.contains("transition-duration")
}

/// Typed localization outcome with deterministic fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserLocalizationOutcome {
    /// Rendered value (fallback locale when the selected one lacks it).
    pub browser_value: String,
    /// True when the key missed in the selected locale / family.
    pub browser_missing_key: bool,
}

/// Resolve a simple key through the catalog stack: requested locale →
/// language family → default (`en-US`). Returns the fallback value
/// with `browser_missing_key = true` when the selected locale lacks
/// the key; returns the key itself when no catalog has it.
pub fn browser_resolve_text(locale_tag: &str, key: &str) -> BrowserLocalizationOutcome {
    let locale = Locale::new(locale_tag).unwrap_or_default();
    let resolver = CatalogResolver::new(openpanel_app::i18n::default_catalog());
    match resolver.resolve(&locale, key) {
        Some(message) => {
            // The bundled resolver only ships the default catalog, so
            // a non-default locale that resolves did so via fallback.
            let fell_back = locale.as_str() != Locale::default().as_str();
            BrowserLocalizationOutcome {
                browser_value: message.value.clone(),
                browser_missing_key: fell_back && missing_then_fallback(&locale, key),
            }
        }
        None => BrowserLocalizationOutcome {
            browser_value: key.to_string(),
            browser_missing_key: true,
        },
    }
}

/// Render a key with `{name}` substitution and plural selection. The
/// `count` arg selects the CLDR plural form when the key is a plural
/// entry; otherwise the simple template renders.
pub fn browser_render_text(
    locale_tag: &str,
    key: &str,
    args: &BTreeMap<String, String>,
) -> BrowserLocalizationOutcome {
    let locale = Locale::new(locale_tag).unwrap_or_default();
    let resolver = CatalogResolver::new(openpanel_app::i18n::default_catalog());
    if let Some(message) = resolver.resolve(&locale, key) {
        return BrowserLocalizationOutcome {
            browser_value: render_template(&message.value, args),
            browser_missing_key: locale.as_str() != Locale::default().as_str()
                && missing_then_fallback(&locale, key),
        };
    }
    if let Some(forms) = resolver.resolve_plural(&locale, key) {
        let count: u64 = args.get("count").and_then(|c| c.parse().ok()).unwrap_or(0);
        let selected = forms.select(&locale, count);
        return BrowserLocalizationOutcome {
            browser_value: render_template(selected, args),
            browser_missing_key: locale.as_str() != Locale::default().as_str(),
        };
    }
    BrowserLocalizationOutcome {
        browser_value: render_template(key, args),
        browser_missing_key: true,
    }
}

/// Locales the panel shell accepts (mirrors the settings allowlist).
pub fn browser_supported_locales() -> Vec<String> {
    vec!["en-US".to_string(), "zh-CN".to_string()]
}

/// Format a number with the locale's decimal / grouping rules.
pub fn browser_format_number(locale_tag: &str, value: f64) -> String {
    let locale = Locale::new(locale_tag).unwrap_or_default();
    Formatter::new().format_number(&locale, value)
}

/// Format a currency amount with the locale's position rules.
pub fn browser_format_currency(locale_tag: &str, value: f64, code: &str) -> String {
    let locale = Locale::new(locale_tag).unwrap_or_default();
    Formatter::new().format_currency(&locale, value, code)
}

/// Format a calendar date with the locale's order / separator rules.
pub fn browser_format_date(locale_tag: &str, year: i32, month: u32, day: u32) -> String {
    let locale = Locale::new(locale_tag).unwrap_or_default();
    Formatter::new().format_date(&locale, year, month, day)
}

/// Format a time with the locale's 12/24-hour rules.
pub fn browser_format_time(locale_tag: &str, hour: u32, minute: u32) -> String {
    let locale = Locale::new(locale_tag).unwrap_or_default();
    Formatter::new().format_time(&locale, hour, minute)
}

/// The text-direction attribute for a locale (`ltr` / `rtl`).
pub fn browser_text_dir(locale_tag: &str) -> &'static str {
    let locale = Locale::new(locale_tag).unwrap_or_default();
    Formatter::new().dir(&locale)
}

/// Sanitize a route + viewport + check into an artifact filename.
pub fn browser_artifact_filename(route: &str, viewport: u32, check: &str) -> String {
    let mut slug = route
        .trim_start_matches('/')
        .replace('/', "_")
        .replace(['{', '}'], "");
    if slug.is_empty() {
        slug = "index".to_string();
    }
    format!("{slug}@{viewport}-{check}.html")
}

/// Write a failure artifact (rendered HTML) for diagnosis. Creates
/// the directory when missing.
pub fn browser_write_artifact(
    dir: &std::path::Path,
    filename: &str,
    body: &str,
) -> Result<std::path::PathBuf, std::io::Error> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(filename);
    std::fs::write(&path, body)?;
    Ok(path)
}

fn missing_then_fallback(locale: &Locale, key: &str) -> bool {
    // The bundled resolver only ships the default catalog, so a
    // non-default locale that resolves did so via fallback.
    locale.as_str() != Locale::default().as_str()
        && openpanel_app::i18n::default_catalog().get(key).is_some()
}

fn count_tag(html: &str, tag: &str) -> usize {
    let open = format!("<{tag}");
    let mut count = 0;
    let mut cursor = 0;
    while let Some(idx) = html[cursor..].find(open.as_str()) {
        let abs = cursor + idx;
        let after = &html[abs + open.len()..];
        match after.chars().next() {
            Some(' ') | Some('>') => count += 1,
            _ => {}
        }
        cursor = abs + open.len();
    }
    count
}

fn tag_inner_text(html: &str, open_start: usize, tag: &str) -> String {
    let close_tag = format!("</{tag}>");
    let Some(open_end) = html[open_start..].find('>') else {
        return String::new();
    };
    let body_start = open_start + open_end + 1;
    let Some(close) = html[body_start..].find(close_tag.as_str()) else {
        return String::new();
    };
    html[body_start..body_start + close].to_string()
}

fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for ch in text.chars() {
        match ch {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    out
}

fn nameless_interactives(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in ["button", "a"] {
        let open = format!("<{tag}");
        let mut cursor = 0;
        while let Some(idx) = html[cursor..].find(open.as_str()) {
            let abs = cursor + idx;
            let after = &html[abs + open.len()..];
            let valid = matches!(after.chars().next(), Some(' ') | Some('>'));
            if !valid {
                cursor = abs + open.len();
                continue;
            }
            let Some(open_end) = html[abs..].find('>') else {
                break;
            };
            let opening = &html[abs..abs + open_end];
            let has_label =
                opening.contains("aria-label=\"") || opening.contains("aria-labelledby=\"");
            let text = strip_tags(&tag_inner_text(html, abs, tag));
            if !has_label && text.trim().is_empty() {
                let snippet: String = opening.chars().take(80).collect();
                out.push(format!("<{tag} {snippet}>"));
            }
            cursor = abs + open_end + 1;
        }
    }
    out
}

fn input_tag_is_visible(tag: &str) -> bool {
    let kind = attr_value(tag, "type").unwrap_or("text");
    kind != "hidden" && kind != "submit" && kind != "button"
}

fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let idx = tag.find(needle.as_str())?;
    let rest = &tag[idx + needle.len()..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn unlabelled_inputs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(idx) = html[cursor..].find("<input") {
        let abs = cursor + idx;
        let after = &html[abs..];
        let Some(close) = after.find('>') else {
            break;
        };
        let tag = &after[..close + 1];
        cursor = abs + "<input".len();
        if !input_tag_is_visible(tag) {
            continue;
        }
        let labelled = match attr_value(tag, "id") {
            Some(id) => {
                html.contains(format!("for=\"{id}\"").as_str()) || wrapped_in_label(html, abs)
            }
            None => wrapped_in_label(html, abs),
        };
        if !labelled {
            let snippet: String = tag.chars().take(80).collect();
            out.push(snippet);
        }
    }
    out
}

fn wrapped_in_label(html: &str, input_pos: usize) -> bool {
    let preceding = &html[..input_pos];
    let Some(label_open) = preceding.rfind("<label") else {
        return false;
    };
    !preceding[label_open..].contains("</label>")
}

fn has_fixed_inline_width(html: &str) -> bool {
    for chunk in html.split("style=\"") {
        let Some(end) = chunk.find('"') else {
            continue;
        };
        let style = chunk[..end].to_ascii_lowercase();
        let Some(idx) = style.find("width:") else {
            continue;
        };
        let after = style[idx + "width:".len()..].trim_start();
        let numeric_end = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .count();
        let (num, rest) = after.split_at(numeric_end);
        if num.is_empty() {
            continue;
        }
        if rest.trim_start().starts_with('%') {
            continue;
        }
        return true;
    }
    false
}

fn count_bare_tables(html: &str) -> usize {
    let mut count = 0;
    let mut cursor = 0;
    while let Some(idx) = html[cursor..].find("<table") {
        let abs = cursor + idx;
        let after = &html[abs..];
        let Some(close) = after.find('>') else {
            break;
        };
        let tag = &after[..close + 1];
        if !tag.contains("class=\"") {
            count += 1;
        }
        cursor = abs + close + 1;
    }
    count
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const CAPABILITY: &str = "browser-ui-quality";

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(BROWSER_UI_QUALITY_CAPABILITY, CAPABILITY);
        assert_eq!(CAPABILITY, "browser-ui-quality");
    }

    #[test]
    fn viewports_cover_mobile_tablet_desktop() {
        assert_eq!(browser_quality_viewports(), vec![360, 768, 1280]);
    }

    #[test]
    fn concrete_paths_substitute_smoke_values() {
        assert_eq!(
            browser_concrete_path("/sites/{id}/domains"),
            "/sites/smoke-site/domains"
        );
        assert_eq!(
            browser_concrete_path("/sites/{site_id}/files"),
            "/sites/smoke-site/files"
        );
        assert_eq!(browser_concrete_path("/ssl/{domain}"), "/ssl/example.com");
        assert_eq!(browser_concrete_path("/sites"), "/sites");
    }

    #[test]
    fn matrix_derives_from_registry() {
        let paths = browser_matrix_paths();
        assert_eq!(paths.len(), crate::capability_registry::entries().len());
        assert!(paths.contains(&"/".to_string()));
        assert!(paths.contains(&"/audit".to_string()));
        assert!(paths.contains(&"/sites/smoke-site/domains".to_string()));
    }

    #[test]
    fn role_matrix_filters_owner_only_routes() {
        let owner = browser_paths_for_role(Role::Owner);
        let user = browser_paths_for_role(Role::User);
        assert!(owner.contains(&"/audit".to_string()));
        assert!(!user.contains(&"/audit".to_string()));
        assert!(user.contains(&"/sites".to_string()));
    }

    #[test]
    fn accessibility_flags_nameless_and_unlabelled() {
        let html = "<html lang=\"en-US\"><h1>Title</h1>\
            <button></button><a href=\"/x\"></a>\
            <label>name<input type=\"text\" id=\"n\"></label></html>";
        let report = browser_evaluate_accessibility(html);
        assert_eq!(report.browser_nameless_interactives.len(), 2);
        // The labelled input is wrapped, so only lang/h1 pass.
        assert!(report.browser_unlabelled_inputs.is_empty());
        assert!(!browser_accessibility_is_clean(&report));
    }

    #[test]
    fn accessibility_accepts_named_shell() {
        let html = "<html lang=\"en-US\"><h1>Dashboard</h1>\
            <button aria-label=\"Toggle\">x</button><a href=\"/\">Home</a>\
            <label for=\"q\">Q</label><input id=\"q\" type=\"text\"></html>";
        let report = browser_evaluate_accessibility(html);
        assert!(browser_accessibility_is_clean(&report));
    }

    #[test]
    fn responsive_flags_fixed_width_and_bare_tables() {
        let html = "<div class=\"layout\"><table><tr><td>x</td></tr></table>\
            <div style=\"width: 200px\"></div></div>";
        let report = browser_evaluate_responsive(html);
        assert!(report.browser_has_fixed_inline_width);
        assert_eq!(report.browser_bare_tables, 1);
        assert!(!browser_responsive_is_clean(&report));
    }

    #[test]
    fn responsive_accepts_fluid_shell() {
        let html = "<div class=\"layout\"><table class=\"table\"></table>\
            <div style=\"width: 50%\"></div></div>";
        let report = browser_evaluate_responsive(html);
        assert!(browser_responsive_is_clean(&report));
    }

    #[test]
    fn focus_and_motion_probes_match_contract() {
        let css = ":focus-visible { outline: 2px solid var(--op-color-focus-ring); } \
            @media (prefers-reduced-motion: reduce) { * { animation-duration: 0s; transition-duration: 0s; } }";
        assert!(browser_focus_ring_is_present(css));
        assert!(browser_reduced_motion_is_present(css));
        assert!(!browser_focus_ring_is_present("a { color: red; }"));
        assert!(!browser_reduced_motion_is_present("a { color: red; }"));
    }

    #[test]
    fn localization_falls_back_with_missing_signal() {
        let hit = browser_resolve_text("en-US", "save_button");
        assert_eq!(hit.browser_value, "Save");
        assert!(!hit.browser_missing_key);
        let fallback = browser_resolve_text("fr-FR", "save_button");
        assert_eq!(fallback.browser_value, "Save");
        assert!(fallback.browser_missing_key);
        let absent = browser_resolve_text("en-US", "no_such_key_xyz");
        assert_eq!(absent.browser_value, "no_such_key_xyz");
        assert!(absent.browser_missing_key);
    }

    #[test]
    fn localization_renders_placeholders_and_plurals() {
        let mut args = BTreeMap::new();
        args.insert("name".to_string(), "Ada".to_string());
        let out = browser_render_text("en-US", "welcome", &args);
        assert_eq!(out.browser_value, "Welcome, Ada!");
        let mut count = BTreeMap::new();
        count.insert("count".to_string(), "1".to_string());
        let one = browser_render_text("en-US", "files_count", &count);
        assert_eq!(one.browser_value, "1 file");
        count.insert("count".to_string(), "4".to_string());
        let many = browser_render_text("en-US", "files_count", &count);
        assert_eq!(many.browser_value, "4 files");
    }

    #[test]
    fn formatting_helpers_delegate_to_domain_rules() {
        assert_eq!(browser_format_number("de-DE", 1234.5), "1.234,5");
        assert_eq!(browser_format_date("de-DE", 2026, 9, 13), "13.09.2026");
        assert_eq!(browser_text_dir("ar-SA"), "rtl");
        assert_eq!(browser_text_dir("en-US"), "ltr");
        assert!(!browser_format_currency("en-US", 10.0, "USD").is_empty());
        assert!(!browser_format_time("en-US", 13, 5).is_empty());
    }

    #[test]
    fn artifact_names_and_writes_round_trip() {
        let name = browser_artifact_filename("/sites/smoke-site/domains", 360, "axe");
        assert_eq!(name, "sites_smoke-site_domains@360-axe.html");
        let dir = std::env::temp_dir().join("browser-ui-quality-test");
        let path = browser_write_artifact(&dir, &name, "<html></html>").expect("write");
        assert!(path.exists());
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }
}
