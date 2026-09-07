//! Static / contract-level assertions that enforce the web UI styling
//! & responsive layout contract (OpenSpec
//! `add-web-ui-styling-and-responsive-layout`).  These are plain
//! string scans over the static assets — they make CI fail loudly
//! if anyone re-introduces a regression.
#![cfg(test)]

const APP_CSS: &[u8] = include_bytes!("../assets/app.css");
const TOKENS_CSS: &[u8] = include_bytes!("../assets/tokens.css");

fn css(b: &[u8]) -> &str {
    std::str::from_utf8(b).expect("assets are valid utf-8")
}

#[test]
fn app_css_has_no_max_width_media_queries() {
    for line in css(APP_CSS).lines() {
        assert!(
            !line.contains("(max-width:"),
            "max-width media queries are forbidden by the contract: {line}"
        );
    }
}

#[test]
fn app_css_uses_only_min_width_breakpoints_per_contract() {
    // The contract mandates three breakpoints: 768 px and 1280 px.
    let css = css(APP_CSS);
    assert!(
        css.contains("@media (min-width: 768px)"),
        "missing 768px breakpoint"
    );
    assert!(
        css.contains("@media (min-width: 1280px)"),
        "missing 1280px breakpoint"
    );
}

#[test]
fn app_css_tokenises_spacing_outside_tokens() {
    // Spot-check: the legacy literal `padding: 1.5rem` is gone in favour
    // of `var(--op-space-*)`.  Make sure the most common spacing props
    // in the dashboard / sidebar rules source tokens.
    let css = css(APP_CSS);
    for needle in [
        ".topbar {",
        ".sidebar {",
        ".card {",
        ".gauge {",
        "form.form {",
    ] {
        let block_idx = css.find(needle).expect(needle);
        let tail = &css[block_idx..];
        let end = tail.find('}').unwrap_or(tail.len());
        let block = &tail[..end];
        assert!(
            block.contains("var(--op-"),
            "{needle} block must source tokens: {block}"
        );
    }
}

#[test]
fn app_css_defines_focus_visible_ring() {
    let css = css(APP_CSS);
    assert!(
        css.contains("--op-color-focus-ring"),
        "tokens.css missing focus ring variable"
    );
    assert!(
        css.contains("outline: 2px solid var(--op-color-focus-ring)"),
        "global focus-visible ring rule missing from app.css"
    );
}

#[test]
fn app_css_bare_form_baseline_is_safe() {
    // A bare <form> (no class) must not fall back to UA defaults —
    // the global `form { ... }` baseline guarantees a sane layout
    // instead.
    let css = css(APP_CSS);
    assert!(
        css.contains("form label {"),
        "form label baseline missing — bare <form>s would fall back to UA defaults"
    );
    assert!(
        css.contains("form input,\nform input[type=\"email\"],"),
        "form input baseline missing"
    );
}

#[test]
fn app_css_plain_anchor_baseline_is_safe() {
    // A bare <a> must source its colour from tokens rather than the
    // browser's default blue underline.
    let css = css(APP_CSS);
    let a_block = css
        .find("a {\n  color: var(--op-color-accent);")
        .or_else(|| css.find("a {\n  color:var(--op-color-accent);"))
        .expect("plain <a> baseline must use --op-color-accent");
    assert!(
        css[a_block..].contains("text-decoration: none"),
        "plain <a> baseline must suppress the UA underline"
    );
}

#[test]
fn app_css_plain_lists_baseline_is_safe() {
    // Plain <ul>/<ol> must drop the UA-default bullet and indent
    // using a token; otherwise lists like the Logs source list show
    // bold browser default dots.
    let css = css(APP_CSS);
    assert!(
        css.contains("ul,\nol {"),
        "plain <ul>/<ol> baseline missing"
    );
    assert!(
        css.contains("list-style: none;"),
        "list bullet reset missing"
    );
}

#[test]
fn tokens_css_is_intact() {
    let tokens = css(TOKENS_CSS);
    for needle in [
        "--op-color-bg",
        "--op-color-fg",
        "--op-color-accent",
        "--op-space-2",
        "--op-radius-md",
        "--op-font-size-md",
    ] {
        assert!(tokens.contains(needle), "tokens.css missing {needle}");
    }
}

// =====================================================================
//  OpenSpec `add-web-ui-element-baseline` (2026-09-07)
//  Static contracts for the global element baseline, the keyboard
//  focus ring coverage, and the reduced-motion block. These are
//  plain string scans: if a future PR deletes or renames the block,
//  CI fails loudly.
// =====================================================================

/// Every block in the new element baseline MUST appear in `app.css`
/// so a route never falls through to UA defaults. The needles are
/// chosen to match the *new* block exactly (line-start anchored or
/// otherwise unique) so the test does not get a false positive from
/// an existing class-scoped rule.
#[test]
fn app_css_global_element_baseline_is_present() {
    let css = css(APP_CSS);
    for needle in [
        // Headings are written as a single combined `h1, h2, h3, h4, h5, h6 {`
        // block; the needle matches the opener exactly.
        "h1, h2, h3, h4, h5, h6 {",
        "p {",
        "table {",
        // `dl, dt, dd` is the typical pattern; the opener is unique.
        "dl, dt, dd {",
        "pre {",
        // The bare `code` baseline is its own block, distinct from
        // the existing `code, kbd, pre, samp` block.
        "\ncode {",
        "th, td {",
        "hr {",
        "fieldset {",
        "legend {",
        "blockquote {",
        "figure {",
        "img {",
    ] {
        assert!(
            css.contains(needle),
            "app.css missing element-baseline rule for `{needle}` — \
             bare elements would fall through to UA defaults"
        );
    }
}

/// Each new element-baseline block MUST source its values from
/// `var(--op-*)` rather than literal `rem`/`px`/hex values. The
/// `prefers-reduced-motion` block is exempt (it is a media-query
/// envelope, not a visual rule).
#[test]
fn app_css_element_baseline_blocks_source_tokens() {
    let css = css(APP_CSS);
    for needle in [
        "h1, h2, h3, h4, h5, h6 {",
        "p {",
        "table {",
        "dl, dt, dd {",
        "pre {",
        "th, td {",
    ] {
        let block_idx = css
            .find(needle)
            .unwrap_or_else(|| panic!("missing {needle}"));
        let tail = &css[block_idx..];
        let end = tail.find('}').unwrap_or(tail.len());
        let block = &tail[..end];
        assert!(
            block.contains("var(--op-"),
            "{needle} block must source tokens (var(--op-*)): {block}"
        );
    }
}

/// Every interactive element the user can reach with the keyboard
/// MUST have a `:focus-visible` rule. This list mirrors the
/// `web-ui-styling` "Keyboard Focus Ring Coverage" requirement.
#[test]
fn app_css_focus_visible_covers_all_interactives() {
    let css = css(APP_CSS);
    // The plain-anchor baseline is at `app.css:174`; the form
    // baseline is at `app.css:689`. We assert both still exist.
    assert!(
        css.contains("a:focus-visible"),
        "plain <a> :focus-visible rule missing"
    );
    assert!(
        css.contains("form input:focus-visible"),
        "form input :focus-visible rule missing"
    );
    assert!(
        css.contains("form button:focus-visible"),
        "form button :focus-visible rule missing"
    );

    // The non-form interactives that were not covered before the
    // change — pin each so a future regression lights up the build.
    for selector in [
        ".topbar button:focus-visible",
        ".table button:focus-visible",
        ".btn:focus-visible",
        ".button:focus-visible",
        ".nav-item:focus-visible",
        ".nav-rail-toggle:focus-visible",
        ".op-modal-close:focus-visible",
    ] {
        assert!(
            css.contains(selector),
            "missing :focus-visible rule for `{selector}` — \
             keyboard users would have no visible focus indicator"
        );
    }
}

/// The reduced-motion media query MUST exist and MUST silence the
/// spinner animation and the transition durations.
#[test]
fn app_css_respects_prefers_reduced_motion() {
    let css = css(APP_CSS);
    let block_idx = css
        .find("@media (prefers-reduced-motion: reduce)")
        .expect("app.css missing @media (prefers-reduced-motion: reduce) block");
    let tail = &css[block_idx..];
    // Bound the block to the next top-level `@` or end of file.
    let end = tail[1..]
        .find("@media")
        .map(|i| i + 1)
        .unwrap_or(tail.len());
    let block = &tail[..end];
    assert!(
        block.contains("animation-duration"),
        "reduced-motion block must set animation-duration: {block}"
    );
    assert!(
        block.contains("transition-duration"),
        "reduced-motion block must set transition-duration: {block}"
    );
}

/// Static source-level guard: every maud template `<table>` in
/// `crates/openpanel-web/src/` MUST declare a class. Per-resource
/// routes (`/sites/{id}/ftp`, `/settings/tokens`, `/cron`,
/// `/sites/{id}/security`) are not in `PUBLIC_ROUTES` so the
/// runtime walk cannot reach them; this unit test picks up the
/// regression at compile time instead.
#[test]
fn every_maud_table_has_a_class() {
    use std::path::Path;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<(String, String)> = Vec::new();
    let entries = std::fs::read_dir(&dir).expect("read src dir");
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "rs" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Detect maud `table {` blocks that are NOT preceded by a
        // class attribute. The check is conservative: a line that
        // starts with optional whitespace and `table` followed by
        // whitespace and `{` is a bare element block.
        for (lineno, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("table ") && trimmed.ends_with('{') {
                // The previous non-blank line should be the maud
                // selector for `<table class="...">`. Maud requires
                // the class attribute on the same line as the tag,
                // so the `class="..."` substring is on the same
                // physical line as `table` for a non-bare block.
                if !line.contains("class=") {
                    let file = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("?")
                        .to_string();
                    offenders.push((format!("{file}:{}", lineno + 1), line.to_string()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "maud `<table {{` blocks must declare a class. Offenders:\n{}",
        offenders
            .iter()
            .map(|(loc, src)| format!("  {loc}: {src}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
