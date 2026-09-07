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

// =====================================================================
//  OpenSpec `resolve-unstyled-ui-classes` (2026-09-07)
//  Static contracts for class-coverage: every literal class token in
//  `crates/openpanel-web/src/` MUST have a rule in `app.css`. Dynamic
//  families concatenated at render time (e.g. `audit-row audit-outcome-{x}`)
//  are expanded via a fixture table so a future variant added without a
//  rule fails the build. The CSS must also not declare the same class
//  selector twice — the historic `.card` × 2 bug is the canonical
//  regression.
// =====================================================================

/// Class families that are concatenated at render time via
/// `format!("class-{x}", variant)`. The fixture table pins every
/// concrete variant the codebase can produce; adding a new variant
/// here without a matching rule fails the static contract test.
const DYNAMIC_CLASS_FAMILIES: &[(&str, &[&str])] = &[
    // audit-row is the base class; the outcome suffix is the variant.
    (
        "audit-row",
        &[
            "audit-row",
            "audit-row audit-outcome-success",
            "audit-row audit-outcome-failure",
            "audit-row audit-outcome-denied",
        ],
    ),
    (
        "audit-badge",
        &[
            "audit-badge",
            "audit-badge audit-badge-success",
            "audit-badge audit-badge-failure",
            "audit-badge audit-badge-denied",
        ],
    ),
    // host_fleet.rs uses `class="status status-{lowercase}"` against
    // a free-form string (Online/Offline/Pending/Revoked).
    (
        "host-status",
        &[
            "status status-online",
            "status status-offline",
            "status status-pending",
            "status status-revoked",
        ],
    ),
    // dashboard/mod.rs: ResourceStatus::class() returns one of these.
    // `gauge-fill` / `gauge-value` / `disk-status` are emitted with the
    // `op-status-*` token appended; `gauge-status` uses a plain span.
    (
        "resource-status",
        &[
            "op-status-healthy",
            "op-status-degraded",
            "op-status-unknown",
            "op-status-error",
            "gauge-fill op-status-healthy",
            "gauge-fill op-status-degraded",
            "gauge-fill op-status-unknown",
            "gauge-fill op-status-error",
            "gauge-value op-status-healthy",
            "gauge-value op-status-degraded",
            "gauge-value op-status-unknown",
            "gauge-value op-status-error",
            "gauge-status op-status-healthy",
            "gauge-status op-status-degraded",
            "gauge-status op-status-unknown",
            "gauge-status op-status-error",
            "disk-status op-status-healthy",
            "disk-status op-status-degraded",
            "disk-status op-status-unknown",
            "disk-status op-status-error",
        ],
    ),
    // dashboard/mod.rs: literal "op-status-stale" or "op-status-fresh".
    (
        "snapshot-freshness",
        &["op-status-fresh", "op-status-stale"],
    ),
    // dashboard/mod.rs: Severity::class().
    (
        "attention-severity",
        &["op-attn-critical", "op-attn-warning", "op-attn-info"],
    ),
    // software_center_trust.rs: DigestState::class().
    ("trust-state", &["trust", "trust--ok", "trust--blocked"]),
    // layer.rs: op-toast with kind suffix.
    (
        "toast-kind",
        &[
            "op-toast op-toast--success",
            "op-toast op-toast--error",
            "op-toast op-toast--info",
            "op-toast op-toast--warning",
        ],
    ),
];

/// Walk every literal `class="..."` attribute in
/// `crates/openpanel-web/src/*.rs` and return the unique set of
/// class tokens. Multi-token class attributes are split on
/// whitespace. Returns `Vec<(file, line, token)>` so error messages
/// point at the source location.
fn collect_literal_class_tokens() -> Vec<(String, usize, String)> {
    use std::path::Path;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out: Vec<(String, usize, String)> = Vec::new();
    let entries = std::fs::read_dir(&dir).expect("read src dir");
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "rs" {
            continue;
        }
        // Skip this file itself: it is the test that asserts the
        // contract, so its `class="..."` literals are fixture
        // strings, not template output.
        let file = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        if file == "web_ui_styling.rs" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (lineno, line) in text.lines().enumerate() {
            // Find every `class="..."` literal on the line. Maud
            // templates are line-based, and maud's `class="a b"`
            // form is a single physical line.
            let mut cursor = 0;
            while let Some(idx) = line[cursor..].find("class=\"") {
                let abs = cursor + idx + "class=\"".len();
                let Some(end) = line[abs..].find('"') else {
                    break;
                };
                let raw = &line[abs..abs + end];
                for tok in raw.split_whitespace() {
                    if !tok.is_empty() {
                        out.push((file.clone(), lineno + 1, tok.to_string()));
                    }
                }
                cursor = abs + end + 1;
            }
        }
    }
    out
}

/// Parse `app.css` and return the unique set of class names that
/// appear as a rule selector. A class selector is any `.ident`
/// segment in the prelude of a `{ ... }` block. The prelude is
/// everything from the end of the previous block to the next `{`.
fn collect_defined_class_names(css: &str) -> std::collections::BTreeSet<String> {
    let mut defined = std::collections::BTreeSet::new();
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the next top-level `{` (the one that starts a rule
        // block). We rely on the fact that `app.css` never has a
        // literal `{` inside a string — a single `grep '"[^{]*{'`
        // over the asset confirms this. Simpler and more robust
        // than string-aware tracking, which previously got
        // confused by apostrophes in comments.
        let mut block_start = None;
        let mut j = i;
        while j < bytes.len() {
            if bytes[j] == b'{' {
                block_start = Some(j);
                break;
            }
            j += 1;
        }
        let Some(start) = block_start else {
            break;
        };
        let prelude = &css[i..start];
        // Split prelude on `,` for grouped selectors. Then, for each
        // selector, extract every `.classname` substring (a class
        // can appear anywhere in the selector — `.table td.actions`
        // defines both `table` and `actions`). A class name is the
        // run of identifier characters after a `.`; it may be
        // followed by a pseudo-class (`:`), pseudo-element (`::`),
        // a descendant combinator, or a modifier (`--`).
        for selector in prelude.split(',') {
            let mut s = selector;
            while let Some(idx) = s.find('.') {
                let after = &s[idx + 1..];
                // The class name runs until a non-identifier char.
                let end = after
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                    .unwrap_or(after.len());
                let name = &after[..end];
                if !name.is_empty() {
                    defined.insert(name.to_string());
                }
                s = &after[end..];
            }
        }
        // Move past the block.
        i = start + 1;
        // Find the matching `}`. CSS allows nested rule blocks
        // (e.g. inside `@media`), so depth must be tracked.
        let mut depth = 1usize;
        let mut k = i;
        while k < bytes.len() && depth > 0 {
            let b = bytes[k];
            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
            }
            k += 1;
        }
        i = k;
    }
    defined
}

/// Detect duplicate top-level rule openers: the same selector
/// prelude (up to whitespace normalization) declared in two
/// different `{ ... }` blocks. This is how the historic `.card` ×
/// 2 bug surfaces — both blocks open with `.card {`.
fn collect_duplicate_selectors(css: &str) -> Vec<(String, Vec<usize>)> {
    let mut seen: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    let bytes = css.as_bytes();
    let mut i = 0;
    let mut block_no = 0;
    while i < bytes.len() {
        let mut depth = 0usize;
        let mut block_start = None;
        let mut j = i;
        while j < bytes.len() {
            let b = bytes[j];
            if b == b'{' {
                if depth == 0 {
                    block_start = Some(j);
                    break;
                }
                depth += 1;
            } else if b == b'}' && depth > 0 {
                depth = depth.saturating_sub(1);
            }
            j += 1;
        }
        let Some(start) = block_start else {
            break;
        };
        let prelude_raw = &css[i..start];
        // Only top-level rules: the prelude must not itself contain
        // an unmatched `{`. A balanced brace scan over the prelude
        // is the safe check. Top-level preludes (no nested
        // at-rules) never contain `{`.
        let open_in_prelude = prelude_raw.bytes().filter(|b| *b == b'{').count();
        if open_in_prelude == 0 {
            // Normalize: collapse whitespace, trim, lowercase.
            let normalized: String = prelude_raw
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase();
            // Only consider simple class-led preludes (start with
            // `.`); compound selectors that merely share a class
            // token are not duplicates.
            if normalized.starts_with('.') {
                seen.entry(normalized).or_default().push(block_no);
            }
        }
        block_no += 1;
        i = start + 1;
        depth = 1;
        let mut k = i;
        while k < bytes.len() && depth > 0 {
            let b = bytes[k];
            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
            }
            k += 1;
        }
        i = k;
    }
    seen.into_iter()
        .filter(|(_, locs)| locs.len() > 1)
        .collect()
}

/// Every literal `class="..."` token in `crates/openpanel-web/src/`
/// MUST have a rule in `app.css`. A new class that ships in a PR
/// without a rule fails the build.
#[test]
fn every_used_class_token_has_a_rule() {
    let css = css(APP_CSS);
    let defined = collect_defined_class_names(css);
    let mut missing: Vec<(String, usize, String)> = Vec::new();
    for (file, line, token) in collect_literal_class_tokens() {
        if !defined.contains(&token) {
            missing.push((file, line, token));
        }
    }
    assert!(
        missing.is_empty(),
        "the following class tokens are used in source but have no rule in app.css:\n{}",
        missing
            .iter()
            .map(|(f, l, t)| format!("  {f}:{l}: `{t}`"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Every concrete variant in the dynamic class families fixture
/// table MUST have a matching rule in `app.css`. The check uses the
/// SAME set of class selectors the literal test uses; we just split
/// each combined token on whitespace and assert every piece is
/// defined.
#[test]
fn every_dynamic_class_family_variant_has_a_rule() {
    let css = css(APP_CSS);
    let defined = collect_defined_class_names(css);
    let mut missing: Vec<(&str, String, Vec<String>)> = Vec::new();
    for (family, variants) in DYNAMIC_CLASS_FAMILIES {
        for variant in *variants {
            let mut local_missing = Vec::new();
            for tok in variant.split_whitespace() {
                if !defined.contains(tok) {
                    local_missing.push(tok.to_string());
                }
            }
            if !local_missing.is_empty() {
                missing.push((family, variant.to_string(), local_missing));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "dynamic class family variants missing rules:\n{}",
        missing
            .iter()
            .map(|(f, v, ms)| format!("  family `{f}` variant `{v}` missing: {}", ms.join(", ")))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// No top-level rule selector MAY appear twice in `app.css`. The
/// historic `.card` × 2 bug is the canonical regression this test
/// guards against — the storefront variant now lives at
/// `.storefront__card` so the dashboard's `.card` rule is unique.
#[test]
fn no_duplicate_class_declarations() {
    let css = css(APP_CSS);
    let dupes = collect_duplicate_selectors(css);
    assert!(
        dupes.is_empty(),
        "the following selectors are declared more than once:\n{}",
        dupes
            .iter()
            .map(|(sel, locs)| format!("  `{sel}` at blocks {locs:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// `<label class="checkbox">` MUST render its checkbox and its text
/// inline. The default `form label` baseline is `flex-direction:
/// column`; `form label.checkbox` overrides that.
#[test]
fn label_checkbox_renders_inline() {
    let css = css(APP_CSS);
    assert!(
        css.contains("form label.checkbox"),
        "app.css missing `form label.checkbox` rule — checkbox labels would \
         stack vertically instead of sitting inline"
    );
    let block_idx = css
        .find("form label.checkbox")
        .expect("form label.checkbox");
    let tail = &css[block_idx..];
    let end = tail.find('}').unwrap_or(tail.len());
    let block = &tail[..end];
    assert!(
        block.contains("flex-direction: row") || block.contains("flex-direction:row"),
        "form label.checkbox must set flex-direction: row, got: {block}"
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
