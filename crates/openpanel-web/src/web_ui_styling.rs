//! Static / contract-level assertions that enforce the web UI styling
//! & responsive layout contract (OpenSpec
//! `add-web-ui-styling-and-responsive-layout`).  These are plain
//! string scans over the static assets — they make CI fail loudly
//! if anyone re-introduces a regression.

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
