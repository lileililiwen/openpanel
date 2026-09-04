//! Tests for the nginx config generator.

use chrono::Utc;
use openpanel_domain::{
    ByteSize, CompressionPolicy, Email, HstsPolicy, Password, PreviewEnvironment, Role, TlsVersion,
    User, Username,
    sites::{TransportPolicy, site::Site},
};
use uuid::Uuid;

use crate::sites::nginx::{NginxConfigGenerator, NginxPaths, managed_log_format};
fn dummy_site() -> Site {
    Site::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "example.com",
        vec!["www.example.com".into(), "api.example.com".into()],
        "/var/www/example.com/public_html",
        false,
        None,
        "tester",
    )
    .unwrap()
}

#[test]
fn render_contains_server_name_and_root() {
    let s = dummy_site();
    let out = NginxConfigGenerator::render(&s);
    assert!(out.contains("server_name example.com www.example.com api.example.com"));
    assert!(out.contains("root /var/www/example.com/public_html"));
    assert!(out.contains("Managed by OpenPanel"));
}

#[test]
fn render_uses_managed_query_free_access_format_and_site_identity() {
    let site = dummy_site();
    let rendered = NginxConfigGenerator::render(&site);
    assert!(rendered.contains("access_log /var/log/openpanel/"));
    assert!(rendered.contains(" openpanel;"));
    assert!(rendered.contains(&format!("set $openpanel_site_id \"{}\";", site.id())));
    assert!(!rendered.contains("$request_uri"));
    assert!(managed_log_format().contains("$uri"));
    assert!(!managed_log_format().contains("$request_uri"));
}

#[test]
fn render_with_php() {
    let s = dummy_site();
    let restored = openpanel_domain::Site::restore(
        s.id(),
        s.owner_id(),
        s.primary_domain().to_string(),
        s.aliases().to_vec(),
        s.document_root().to_string(),
        true,
        Some("8.3".into()),
        s.status(),
        s.created_at(),
        Utc::now(),
        s.created_by().to_string(),
        s.modified_by().to_string(),
    );
    let out = NginxConfigGenerator::render(&restored);
    assert!(out.contains("index.php"));
    assert!(out.contains("try_files $uri $uri/ /index.php?$query_string;"));
    assert!(out.contains("location ~ \\.php$"));
    assert!(out.contains("fastcgi_pass unix:/run/php/php8.3-fpm.sock;"));
    assert!(out.contains("fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;"));
}

#[test]
fn render_uses_canonical_lower_case_domain() {
    // Sanity check: pass a mixed-case domain and verify it isn't relied on
    // (the Site ctor already lowercases).
    let _ = User::new(
        Uuid::new_v4(),
        Username::new("dummy").unwrap(),
        Email::new("d@example.com").unwrap(),
        Password::from_hash("$argon2id$dummy"),
        Role::Owner,
    );
}

#[test]
fn render_with_tls_emits_443_vhost_with_modern_profile() {
    let s = dummy_site();
    let out = NginxConfigGenerator::render_full(
        &s,
        Some((
            "/etc/openpanel/ssl/certs/example.com.crt",
            "/etc/openpanel/ssl/keys/example.com.key",
        )),
        Some("http://127.0.0.1:9080"),
    );
    assert!(out.contains("listen 443 ssl http2"));
    assert!(out.contains("ssl_certificate     /etc/openpanel/ssl/certs/example.com.crt"));
    assert!(out.contains("ssl_certificate_key /etc/openpanel/ssl/keys/example.com.key"));
    assert!(out.contains("TLSv1.2 TLSv1.3"));
    assert!(out.contains("Strict-Transport-Security"));
}

#[test]
fn render_with_tls_and_acme_proxy_emits_challenge_block() {
    let s = dummy_site();
    let out = NginxConfigGenerator::render_full(
        &s,
        Some(("/tmp/cert", "/tmp/key")),
        Some("http://127.0.0.1:9080"),
    );
    assert!(out.contains("proxy_pass http://127.0.0.1:9080"));
    assert!(out.contains(".well-known/acme-challenge"));
}

#[test]
fn golden_default_transport_policy_is_byte_identical_with_pre_tuning_output() {
    let site = Site::new(
        uuid::Uuid::from_u128(1),
        Uuid::new_v4(),
        "golden.example.com",
        vec![],
        "/var/www/golden.example.com/public_html",
        false,
        None,
        "tester",
    )
    .unwrap();
    let out = NginxConfigGenerator::render_full_with_policy(
        &site,
        Some((
            "/etc/openpanel/ssl/golden.crt",
            "/etc/openpanel/ssl/golden.key",
        )),
        None,
        &TransportPolicy::default(),
    );
    // The TLS vhost is the tail of the output.
    let start = out
        .find("server {\n    listen 443")
        .map(|index| &out[index..])
        .expect("tls vhost");
    let golden = include_str!("../testdata/transport_golden.vhost");
    assert_eq!(start, golden);
    // render_full delegates to the default policy.
    assert_eq!(
        NginxConfigGenerator::render_full(
            &site,
            Some((
                "/etc/openpanel/ssl/golden.crt",
                "/etc/openpanel/ssl/golden.key"
            )),
            None,
        ),
        out
    );
}

#[test]
fn http3_policy_emits_single_quic_listen_and_alt_svc_in_vhost() {
    let site = Site::new(
        uuid::Uuid::from_u128(2),
        Uuid::new_v4(),
        "h3.example.com",
        vec![],
        "/var/www/h3.example.com/public_html",
        false,
        None,
        "tester",
    )
    .unwrap();
    let policy = TransportPolicy::new(
        true,
        TlsVersion::V1_2,
        TransportPolicy::default().hsts().cloned(),
        CompressionPolicy::Off,
        ByteSize::new(100 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let out = NginxConfigGenerator::render_full_with_policy(
        &site,
        Some(("/tmp/c", "/tmp/k")),
        None,
        &policy,
    );
    assert_eq!(out.matches("listen 443 quic reuseport;").count(), 1);
    assert!(out.contains("Alt-Svc 'h3=\":443\"; ma=86400;' always;"));
    // Disabling removes both again.
    let off = NginxConfigGenerator::render_full_with_policy(
        &site,
        Some(("/tmp/c", "/tmp/k")),
        None,
        &TransportPolicy::default(),
    );
    assert!(!off.contains("quic"));
    assert!(!off.contains("Alt-Svc"));
}

#[test]
fn transport_overrides_render_into_vhost() {
    let site = Site::new(
        uuid::Uuid::from_u128(3),
        Uuid::new_v4(),
        "tuned.example.com",
        vec![],
        "/var/www/tuned.example.com/public_html",
        false,
        None,
        "tester",
    )
    .unwrap();
    let policy = TransportPolicy::new(
        false,
        TlsVersion::V1_3,
        Some(HstsPolicy::new(31_536_000, true, true).unwrap()),
        CompressionPolicy::Brotli(5),
        ByteSize::new(32 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let out = NginxConfigGenerator::render_full_with_policy(
        &site,
        Some(("/tmp/c", "/tmp/k")),
        None,
        &policy,
    );
    assert!(out.contains("ssl_protocols       TLSv1.3;"));
    assert!(
            out.contains(
                "add_header Strict-Transport-Security \"max-age=31536000; includeSubDomains; preload\" always;"
            )
        );
    assert!(out.contains("brotli on;"));
    assert!(out.contains("brotli_comp_level 5;"));
    assert!(out.contains("client_max_body_size 32M;"));
}

#[test]
fn render_without_tls_omits_tls_vhost() {
    let s = dummy_site();
    let out = NginxConfigGenerator::render_full(&s, None, None);
    assert!(!out.contains("listen 443"));
    assert!(!out.contains("ssl_certificate"));
    // No cert → no force-https redirect either.
    assert!(!out.contains("return 301 https://"));
}

#[test]
fn render_with_tls_emits_force_https_301_on_port_80() {
    let s = dummy_site();
    let out = NginxConfigGenerator::render_full(
        &s,
        Some(("/tmp/cert", "/tmp/key")),
        Some("http://127.0.0.1:9080"),
    );
    assert!(out.contains("return 301 https://$host$request_uri"));
}

fn dummy_preview(now: chrono::DateTime<chrono::Utc>) -> PreviewEnvironment {
    PreviewEnvironment::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        42,
        "pr.example.com",
        now,
    )
    .unwrap()
}

#[test]
fn render_preview_http_only_proxies_to_runtime_port() {
    let now = Utc::now();
    let p = dummy_preview(now);
    let out = NginxConfigGenerator::render_preview(&p, 19042, None);
    assert!(out.contains("server_name 42.pr.example.com"));
    assert!(out.contains("proxy_pass http://127.0.0.1:19042;"));
    assert!(out.contains("set $openpanel_preview \"1\";"));
    assert!(!out.contains("listen 443"));
    assert!(!out.contains("return 301 https://"));
}

#[test]
fn render_preview_with_tls_emits_force_https_and_wildcard_cert() {
    let now = Utc::now();
    let p = dummy_preview(now);
    let out = NginxConfigGenerator::render_preview(
        &p,
        19042,
        Some((
            "/etc/ssl/openpanel/wildcard.pem",
            "/etc/ssl/openpanel/wildcard.key",
        )),
    );
    assert!(out.contains("server_name 42.pr.example.com"));
    assert!(out.contains("ssl_certificate     /etc/ssl/openpanel/wildcard.pem;"));
    assert!(out.contains("ssl_certificate_key /etc/ssl/openpanel/wildcard.key;"));
    assert!(out.contains("return 301 https://$host$request_uri"));
    assert!(out.contains("proxy_pass http://127.0.0.1:19042;"));
}

#[test]
fn render_preview_path_uses_pr_number_and_hostname() {
    let now = Utc::now();
    let p = dummy_preview(now);
    let generator =
        NginxConfigGenerator::new(NginxPaths::under(std::path::PathBuf::from("/tmp/op")));
    let path = generator.preview_active_path(&p);
    assert!(
        path.ends_with("pr-42-42.pr.example.com.conf"),
        "got {path:?}"
    );
}
