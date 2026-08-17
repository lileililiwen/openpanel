use super::*;

fn l(tag: &str) -> Locale {
    Locale::new(tag).expect("locale")
}

#[test]
fn locale_new_normalises() {
    assert_eq!(l("EN-us").as_str(), "en-US");
    assert_eq!(l("zh-CN").as_str(), "zh-CN");
    assert_eq!(l("fr").as_str(), "fr");
}

#[test]
fn locale_rejects_invalid() {
    assert!(Locale::new("").is_err());
    assert!(Locale::new("1").is_err());
    assert!(Locale::new("en-1").is_err());
}

#[test]
fn locale_language_and_region() {
    let en_us = l("en-US");
    assert_eq!(en_us.language(), "en");
    assert_eq!(en_us.region(), Some("US"));
    let fr = l("fr");
    assert_eq!(fr.language(), "fr");
    assert_eq!(fr.region(), None);
}

#[test]
fn catalog_add_and_lookup() {
    let mut catalog = Catalog::new(l("en-US"));
    catalog.add_message("save_button", "Save");
    catalog.add_plural(
        "files_count",
        PluralForms {
            one: Some("1 file".to_string()),
            other: "{count} files".to_string(),
            few: None,
            many: None,
            two: None,
            zero: None,
        },
    );
    assert_eq!(catalog.get("save_button").expect("present").value, "Save");
    assert_eq!(
        catalog.get_plural("files_count").expect("present").other,
        "{count} files"
    );
}

#[test]
fn plural_select_english() {
    let forms = PluralForms {
        one: Some("1 file".to_string()),
        other: "{count} files".to_string(),
        few: None,
        many: None,
        two: None,
        zero: None,
    };
    assert_eq!(forms.select(&l("en-US"), 1), "1 file");
    assert_eq!(forms.select(&l("en-US"), 0), "{count} files");
    assert_eq!(forms.select(&l("en-US"), 5), "{count} files");
}

#[test]
fn plural_select_polish() {
    let forms = PluralForms {
        one: Some("1 plik".to_string()),
        few: Some("pliki".to_string()),
        many: Some("plików".to_string()),
        other: "?".to_string(),
        two: None,
        zero: None,
    };
    assert_eq!(forms.select(&l("pl-PL"), 1), "1 plik");
    assert_eq!(forms.select(&l("pl-PL"), 3), "pliki");
    assert_eq!(forms.select(&l("pl-PL"), 5), "plików");
}

#[test]
fn plural_select_arabic() {
    let forms = PluralForms {
        zero: Some("zero".to_string()),
        one: Some("one".to_string()),
        two: Some("two".to_string()),
        few: Some("few".to_string()),
        many: Some("many".to_string()),
        other: "other".to_string(),
    };
    assert_eq!(forms.select(&l("ar-SA"), 0), "zero");
    assert_eq!(forms.select(&l("ar-SA"), 1), "one");
    assert_eq!(forms.select(&l("ar-SA"), 2), "two");
    assert_eq!(forms.select(&l("ar-SA"), 5), "few");
    assert_eq!(forms.select(&l("ar-SA"), 50), "many");
    assert_eq!(forms.select(&l("ar-SA"), 200), "other");
}

#[test]
fn parse_accept_language_orders_by_quality() {
    let header = "ja-JP, ja;q=0.9, en;q=0.5";
    let tags = parse_accept_language(header);
    assert_eq!(tags, vec!["ja-JP", "ja", "en"]);
}

#[test]
fn negotiator_prefers_url() {
    let n = LocaleNegotiator::new(l("en-US"), vec![l("en-US"), l("fr-FR"), l("zh-CN")]);
    let result = n.negotiate(&NegotiationHints {
        url_locale: Some("zh-CN"),
        user_locale: Some("fr-FR"),
        accept_language: Some("en"),
    });
    assert_eq!(result, l("zh-CN"));
}

#[test]
fn negotiator_prefers_user_over_header() {
    let n = LocaleNegotiator::new(l("en-US"), vec![l("en-US"), l("fr-FR"), l("zh-CN")]);
    let result = n.negotiate(&NegotiationHints {
        url_locale: None,
        user_locale: Some("fr-FR"),
        accept_language: Some("zh-CN"),
    });
    assert_eq!(result, l("fr-FR"));
}

#[test]
fn negotiator_uses_header_with_family_fallback() {
    let n = LocaleNegotiator::new(l("en-US"), vec![l("en-US"), l("fr")]);
    let result = n.negotiate(&NegotiationHints {
        url_locale: None,
        user_locale: None,
        accept_language: Some("fr-FR"),
    });
    assert_eq!(result, l("fr"));
}

#[test]
fn negotiator_falls_back_to_default() {
    let n = LocaleNegotiator::new(l("en-US"), vec![l("en-US")]);
    let result = n.negotiate(&NegotiationHints {
        url_locale: None,
        user_locale: None,
        accept_language: Some("zh-CN"),
    });
    assert_eq!(result, l("en-US"));
}

#[test]
fn resolver_prefers_exact_locale() {
    let mut en = Catalog::new(l("en-US"));
    en.add_message("save", "Save");
    let mut fr = Catalog::new(l("fr-FR"));
    fr.add_message("save", "Enregistrer");
    let mut resolver = CatalogResolver::new(en);
    resolver.add(fr);
    assert_eq!(
        resolver
            .resolve(&l("fr-FR"), "save")
            .expect("present")
            .value,
        "Enregistrer"
    );
}

#[test]
fn resolver_falls_back_to_language_family() {
    let mut en = Catalog::new(l("en-US"));
    en.add_message("save", "Save");
    let mut fr = Catalog::new(l("fr"));
    fr.add_message("save", "Enregistrer");
    let mut resolver = CatalogResolver::new(en);
    resolver.add(fr);
    assert_eq!(
        resolver
            .resolve(&l("fr-FR"), "save")
            .expect("present")
            .value,
        "Enregistrer"
    );
}

#[test]
fn resolver_falls_back_to_default() {
    let mut en = Catalog::new(l("en-US"));
    en.add_message("save", "Save");
    let resolver = CatalogResolver::new(en);
    assert_eq!(
        resolver
            .resolve(&l("de-DE"), "save")
            .expect("present")
            .value,
        "Save"
    );
}

#[test]
fn resolver_returns_none_for_missing_key() {
    let resolver = CatalogResolver::new(Catalog::new(l("en-US")));
    assert!(resolver.resolve(&l("en-US"), "missing").is_none());
}

#[test]
fn formatter_format_number_us() {
    let f = Formatter::new();
    assert_eq!(f.format_number(&l("en-US"), 1234.5), "1,234.5");
}

#[test]
fn formatter_format_number_de() {
    let f = Formatter::new();
    assert_eq!(f.format_number(&l("de-DE"), 1234.5), "1.234,5");
}

#[test]
fn formatter_format_currency_us() {
    let f = Formatter::new();
    assert_eq!(f.format_currency(&l("en-US"), 100.0, "USD"), "USD 100");
}

#[test]
fn formatter_format_currency_de() {
    let f = Formatter::new();
    assert_eq!(f.format_currency(&l("de-DE"), 100.0, "EUR"), "100 EUR");
}

#[test]
fn formatter_format_date_us() {
    let f = Formatter::new();
    assert_eq!(f.format_date(&l("en-US"), 2026, 8, 13), "08-13-2026");
}

#[test]
fn formatter_format_date_de() {
    let f = Formatter::new();
    assert_eq!(f.format_date(&l("de-DE"), 2026, 8, 13), "13.08.2026");
}

#[test]
fn formatter_format_date_zh() {
    let f = Formatter::new();
    assert_eq!(f.format_date(&l("zh-CN"), 2026, 8, 13), "08-13-2026");
}

#[test]
fn formatter_format_time_24h() {
    let f = Formatter::new();
    assert_eq!(f.format_time(&l("de-DE"), 14, 30), "14:30");
}

#[test]
fn formatter_format_time_12h() {
    let f = Formatter::new();
    assert_eq!(f.format_time(&l("en-US"), 14, 30), "2:30 PM");
}

#[test]
fn formatter_rtl() {
    let f = Formatter::new();
    assert_eq!(f.dir(&l("ar-SA")), "rtl");
    assert_eq!(f.dir(&l("en-US")), "ltr");
}

#[test]
fn render_template_substitutes() {
    let mut args = BTreeMap::new();
    args.insert("name".to_string(), "Alice".to_string());
    assert_eq!(render_template("Hello, {name}!", &args), "Hello, Alice!");
}

#[test]
fn render_template_leaves_unknown() {
    let args = BTreeMap::new();
    assert_eq!(render_template("Hi {missing}", &args), "Hi {missing}");
}
