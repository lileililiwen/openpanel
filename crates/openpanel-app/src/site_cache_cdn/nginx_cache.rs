//! nginx cache snippet generator. Produces the `proxy_cache_path`
//! microcache block and per-site cache directives that the sites
//! cap embeds into a vhost. Pure string generation; no I/O.

use openpanel_domain::SiteCachePolicy;

/// Keys-zone name for a site's cache. nginx keys_zone names must
/// be alphanumeric + underscores; we use the site id's hex form.
pub fn keys_zone_name(site_id: uuid::Uuid) -> String {
    format!("z_{}", site_id.simple())
}

/// Render the `proxy_cache_path` http-context directive for a site.
pub fn cache_path_directive(site_id: uuid::Uuid, max_size_mb: u64) -> String {
    format!(
        "proxy_cache_path /var/cache/openpanel/{site_id} levels=1:2 keys_zone={}:1m max_size={}m;",
        keys_zone_name(site_id),
        max_size_mb
    )
}

/// Render the per-location cache directives for a policy.
///
/// Emits `proxy_cache`, `proxy_cache_key`, `proxy_cache_valid`,
/// `proxy_cache_bypass`, and (when enabled) stale-while-revalidate
/// directives. Bypass paths and keyed cookies are folded into the
/// cache key / bypass map expressions.
pub fn cache_directives(policy: &SiteCachePolicy, max_size_mb: u64) -> String {
    let zone = keys_zone_name(policy.site_id());
    let mut out = String::new();
    out.push_str(&cache_path_directive(policy.site_id(), max_size_mb));
    out.push('\n');
    out.push_str(&format!("proxy_cache {zone};\n"));
    out.push_str(&format!(
        "proxy_cache_key \"$scheme$host$request_uri{}\";\n",
        keyed_cookie_suffix(policy)
    ));
    out.push_str(&format!(
        "proxy_cache_valid 200 {}s;\n",
        policy.ttl_seconds()
    ));
    out.push_str("proxy_cache_valid 404 5s;\n");
    if policy.stale_while_revalidate() {
        out.push_str("proxy_cache_background_update on;\n");
    }
    out.push_str("proxy_cache_use_stale error timeout updating;\n");
    if !policy.bypass_paths().is_empty() {
        out.push_str(&format!(
            "proxy_cache_bypass {}{};\n",
            bypass_expr(policy),
            cookie_bypass_expr(policy)
        ));
    }
    out
}

/// Cache-key suffix derived from the keyed cookies. Returns an
/// empty string when no cookies are keyed.
fn keyed_cookie_suffix(policy: &SiteCachePolicy) -> String {
    let keyed = policy.keyed_cookies();
    if keyed.is_empty() {
        String::new()
    } else {
        // "$cookie_name" joins, e.g. &$cookie_session
        let mut s = String::new();
        for c in keyed {
            s.push_str("&$cookie_");
            s.push_str(c);
        }
        s
    }
}

/// Bypass map expression for path patterns.
fn bypass_expr(policy: &SiteCachePolicy) -> String {
    let patterns = policy
        .bypass_paths()
        .iter()
        .map(|p| format!("$uri ~ \"^{}\"", nginx_regex(p)))
        .collect::<Vec<_>>()
        .join(" || ");
    format!("({patterns})")
}

/// Cookie-based bypass expression; empty when no cookie is keyed.
fn cookie_bypass_expr(policy: &SiteCachePolicy) -> String {
    if policy.keyed_cookies().is_empty() {
        String::new()
    } else {
        format!(" || {}", bypass_expr(policy))
    }
}

/// Convert a glob path (e.g. `/wp-admin/*`) to an nginx regex.
/// A single trailing `*` becomes `.*`; other characters are
/// escaped. The result is anchored with `^` and `$`.
fn nginx_regex(pattern: &str) -> String {
    let mut out = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' | '?' => {
                out.push('\\');
                out.push(ch);
            }
            '*' => out.push_str(".*"),
            _ => out.push(ch),
        }
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directive_includes_path_and_zone() {
        let site_id = uuid::Uuid::new_v4();
        let d = cache_path_directive(site_id, 512);
        assert!(d.contains(&site_id.to_string()));
        assert!(d.contains(&format!("max_size=512m")));
        assert!(d.contains("levels=1:2"));
    }

    #[test]
    fn zone_name_is_stable_and_safe() {
        let site_id = uuid::Uuid::new_v4();
        let name = keys_zone_name(site_id);
        assert_eq!(name, keys_zone_name(site_id));
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    }

    #[test]
    fn directives_emit_ttl_and_valid() {
        let policy =
            openpanel_domain::SiteCachePolicy::with_ttl(uuid::Uuid::new_v4(), 120).unwrap();
        let d = cache_directives(&policy, 64);
        assert!(d.contains("proxy_cache_valid 200 120s;"));
        assert!(d.contains("proxy_cache_valid 404 5s;"));
    }

    #[test]
    fn bypass_paths_compile_to_regex_map() {
        let policy = openpanel_domain::SiteCachePolicy::new(uuid::Uuid::new_v4())
            .unwrap()
            .with_bypass_paths(vec!["/wp-admin/*".to_string(), "/api/login".to_string()])
            .unwrap();
        let d = cache_directives(&policy, 64);
        assert!(d.contains("proxy_cache_bypass"));
        assert!(d.contains(r"/wp-admin/.*$"));
        assert!(d.contains(r"/api/login$"));
    }

    #[test]
    fn keyed_cookies_join_cache_key() {
        let policy = openpanel_domain::SiteCachePolicy::new(uuid::Uuid::new_v4())
            .unwrap()
            .with_keyed_cookies(vec!["session".to_string(), "region".to_string()])
            .unwrap();
        let d = cache_directives(&policy, 64);
        assert!(d.contains("&$cookie_session"));
        assert!(d.contains("&$cookie_region"));
    }

    #[test]
    fn stale_while_revalidate_enables_background_update() {
        let policy = openpanel_domain::SiteCachePolicy::new(uuid::Uuid::new_v4())
            .unwrap()
            .with_stale_while_revalidate(true);
        let d = cache_directives(&policy, 64);
        assert!(d.contains("proxy_cache_background_update on;"));
    }
}
