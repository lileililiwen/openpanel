//! Per-site transport tuning: HTTP/3 QUIC toggle, TLS floor, HSTS,
//! compression, and body-size cap. Pure value objects; the nginx
//! renderer in the application layer consumes [`TransportPolicy`].

use serde::{Deserialize, Serialize};

use super::SiteError;

/// Minimum TLS protocol version advertised in `ssl_protocols`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlsVersion {
    /// TLS 1.2 (the panel default floor).
    V1_2,
    /// TLS 1.3 only.
    V1_3,
}

impl TlsVersion {
    /// The nginx token for this version.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V1_2 => "TLSv1.2",
            Self::V1_3 => "TLSv1.3",
        }
    }
}

/// Strict-Transport-Security policy. `None` on the policy removes
/// the header entirely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HstsPolicy {
    max_age_secs: u32,
    include_subdomains: bool,
    preload: bool,
}

impl HstsPolicy {
    /// Construct and validate. Browsers require `preload` to carry a
    /// max-age of at least one year.
    pub fn new(
        max_age_secs: u32,
        include_subdomains: bool,
        preload: bool,
    ) -> Result<Self, SiteError> {
        if preload && max_age_secs < 31_536_000 {
            return Err(SiteError::InvalidTransport(
                "HSTS preload requires max-age >= 31536000".into(),
            ));
        }
        Ok(Self {
            max_age_secs,
            include_subdomains,
            preload,
        })
    }

    /// Render the header value (`max-age=…[; includeSubDomains][; preload]`).
    pub fn render_value(&self) -> String {
        let mut value = format!("max-age={}", self.max_age_secs);
        if self.include_subdomains {
            value.push_str("; includeSubDomains");
        }
        if self.preload {
            value.push_str("; preload");
        }
        value
    }
}

/// Response-body compression policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "level", rename_all = "snake_case")]
pub enum CompressionPolicy {
    /// Emit no compression directives (nginx package defaults apply).
    Off,
    /// gzip with level 1..=9.
    Gzip(u8),
    /// brotli with level 1..=9.
    Brotli(u8),
}

impl CompressionPolicy {
    fn validate_level(level: u8) -> Result<(), SiteError> {
        if !(1..=9).contains(&level) {
            return Err(SiteError::InvalidTransport(format!(
                "compression level must be 1..=9, got {level}"
            )));
        }
        Ok(())
    }

    /// Validate the policy.
    pub fn validate(&self) -> Result<(), SiteError> {
        match self {
            Self::Off => Ok(()),
            Self::Gzip(level) | Self::Brotli(level) => Self::validate_level(*level),
        }
    }

    /// The nginx directive lines (empty for [`CompressionPolicy::Off`]).
    pub fn render_lines(&self) -> String {
        match self {
            Self::Off => String::new(),
            Self::Gzip(level) => format!("    gzip on;\n    gzip_comp_level {level};\n"),
            Self::Brotli(level) => {
                format!("    brotli on;\n    brotli_comp_level {level};\n")
            }
        }
    }
}

/// Body size cap in bytes: 1 KiB ..= 10 GiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ByteSize(u64);

impl ByteSize {
    /// Upper bound.
    pub const MAX: u64 = 10 * 1024 * 1024 * 1024;
    /// Lower bound.
    pub const MIN: u64 = 1024;

    /// Construct and validate.
    pub fn new(bytes: u64) -> Result<Self, SiteError> {
        if !(Self::MIN..=Self::MAX).contains(&bytes) {
            return Err(SiteError::InvalidTransport(format!(
                "body size cap must be {}..={} bytes, got {bytes}",
                Self::MIN,
                Self::MAX
            )));
        }
        Ok(Self(bytes))
    }

    /// Raw byte count.
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// nginx human form: whole MiB → `M`, else whole KiB → `k`,
    /// else raw bytes.
    pub fn render(&self) -> String {
        if self.0.is_multiple_of(1024 * 1024) {
            format!("{}M", self.0 / (1024 * 1024))
        } else if self.0.is_multiple_of(1024) {
            format!("{}k", self.0 / 1024)
        } else {
            self.0.to_string()
        }
    }
}

/// Per-site transport profile rendered into the TLS vhost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportPolicy {
    http3_enabled: bool,
    tls_min_version: TlsVersion,
    hsts: Option<HstsPolicy>,
    compression: CompressionPolicy,
    body_size_cap: ByteSize,
}

impl Default for TransportPolicy {
    /// Byte-identical with the pre-tuning hardcoded profile: HTTP/2
    /// only, TLSv1.2 floor, HSTS `max-age=63072000`, no compression
    /// directives, 100M body cap.
    fn default() -> Self {
        Self {
            http3_enabled: false,
            tls_min_version: TlsVersion::V1_2,
            hsts: Some(HstsPolicy {
                max_age_secs: 63_072_000,
                include_subdomains: false,
                preload: false,
            }),
            compression: CompressionPolicy::Off,
            body_size_cap: ByteSize(100 * 1024 * 1024),
        }
    }
}

impl TransportPolicy {
    /// Construct and validate every part.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        http3_enabled: bool,
        tls_min_version: TlsVersion,
        hsts: Option<HstsPolicy>,
        compression: CompressionPolicy,
        body_size_cap: ByteSize,
    ) -> Result<Self, SiteError> {
        compression.validate()?;
        Ok(Self {
            http3_enabled,
            tls_min_version,
            hsts,
            compression,
            body_size_cap,
        })
    }

    /// Whether HTTP/3 QUIC listeners are emitted.
    pub fn http3_enabled(&self) -> bool {
        self.http3_enabled
    }

    /// Minimum TLS version.
    pub fn tls_min_version(&self) -> TlsVersion {
        self.tls_min_version
    }

    /// HSTS policy, if enabled.
    pub fn hsts(&self) -> Option<&HstsPolicy> {
        self.hsts.as_ref()
    }

    /// Compression policy.
    pub fn compression(&self) -> &CompressionPolicy {
        &self.compression
    }

    /// Body size cap.
    pub fn body_size_cap(&self) -> ByteSize {
        self.body_size_cap
    }

    /// Extra `listen … quic` line (exactly one when enabled).
    pub fn render_quic_listen(&self) -> &str {
        if self.http3_enabled {
            "    listen 443 quic reuseport;\n"
        } else {
            ""
        }
    }

    /// The `Alt-Svc` advertisement line (enabled only with HTTP/3).
    pub fn render_alt_svc(&self) -> &str {
        if self.http3_enabled {
            "    add_header Alt-Svc 'h3=\":443\"; ma=86400;' always;\n"
        } else {
            ""
        }
    }

    /// The `ssl_protocols` value: floor plus TLSv1.3 (deduplicated).
    pub fn render_ssl_protocols(&self) -> String {
        match self.tls_min_version {
            TlsVersion::V1_2 => "TLSv1.2 TLSv1.3".to_string(),
            TlsVersion::V1_3 => "TLSv1.3".to_string(),
        }
    }

    /// The HSTS header line, or empty when disabled.
    pub fn render_hsts(&self) -> String {
        match &self.hsts {
            Some(hsts) => format!(
                "    add_header Strict-Transport-Security \"{}\" always;\n",
                hsts.render_value()
            ),
            None => String::new(),
        }
    }

    /// Compression directive lines.
    pub fn render_compression(&self) -> String {
        self.compression.render_lines()
    }

    /// The `client_max_body_size` line.
    pub fn render_body_size(&self) -> String {
        format!(
            "    client_max_body_size {};\n",
            self.body_size_cap.render()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_matches_pre_tuning_hardcoded_profile() {
        let policy = TransportPolicy::default();
        assert!(!policy.http3_enabled());
        assert_eq!(policy.tls_min_version(), TlsVersion::V1_2);
        assert_eq!(
            policy.hsts().map(HstsPolicy::render_value).as_deref(),
            Some("max-age=63072000")
        );
        assert_eq!(policy.compression(), &CompressionPolicy::Off);
        assert_eq!(policy.body_size_cap().as_u64(), 100 * 1024 * 1024);
        assert_eq!(
            policy.render_body_size(),
            "    client_max_body_size 100M;\n"
        );
        assert_eq!(policy.render_quic_listen(), "");
        assert_eq!(policy.render_alt_svc(), "");
        assert_eq!(policy.render_compression(), "");
    }

    #[test]
    fn test_validation_rejects_invalid_policies() {
        // preload requires one-year max-age.
        assert!(HstsPolicy::new(31_535_999, true, true).is_err());
        assert!(HstsPolicy::new(31_536_000, true, true).is_ok());

        // compression levels outside 1..=9 are rejected.
        assert!(CompressionPolicy::Gzip(0).validate().is_err());
        assert!(CompressionPolicy::Gzip(10).validate().is_err());
        assert!(CompressionPolicy::Brotli(0).validate().is_err());
        assert!(CompressionPolicy::Brotli(9).validate().is_ok());

        // body cap bounds.
        assert!(ByteSize::new(1023).is_err());
        assert!(ByteSize::new(ByteSize::MAX + 1).is_err());
        assert!(ByteSize::new(1024).is_ok());

        // policy construction surfaces validation errors.
        let bad = TransportPolicy::new(
            false,
            TlsVersion::V1_2,
            None,
            CompressionPolicy::Gzip(42),
            ByteSize::new(1024).unwrap(),
        );
        assert!(bad.is_err());
    }

    #[test]
    fn test_http3_emits_single_quic_listen_and_alt_svc() {
        let policy = TransportPolicy::default();
        let enabled = TransportPolicy::new(
            true,
            policy.tls_min_version,
            policy.hsts.clone(),
            policy.compression.clone(),
            policy.body_size_cap,
        )
        .unwrap();
        assert_eq!(
            enabled.render_quic_listen(),
            "    listen 443 quic reuseport;\n"
        );
        assert_eq!(
            enabled.render_alt_svc(),
            "    add_header Alt-Svc 'h3=\":443\"; ma=86400;' always;\n"
        );
        assert_eq!(enabled.render_quic_listen().matches("quic").count(), 1);
        // Disabling removes both again.
        assert_eq!(policy.render_quic_listen(), "");
        assert_eq!(policy.render_alt_svc(), "");
    }

    #[test]
    fn test_tls_floor_and_hsts_rendering() {
        let v13 = TransportPolicy::new(
            false,
            TlsVersion::V1_3,
            Some(HstsPolicy::new(31_536_000, true, true).unwrap()),
            CompressionPolicy::Gzip(6),
            ByteSize::new(1024).unwrap(),
        )
        .unwrap();
        assert_eq!(v13.render_ssl_protocols(), "TLSv1.3");
        assert_eq!(
            v13.render_hsts(),
            "    add_header Strict-Transport-Security \"max-age=31536000; includeSubDomains; preload\" always;\n"
        );
        assert!(v13.render_compression().contains("gzip on;"));
        assert!(v13.render_compression().contains("gzip_comp_level 6;"));
        assert_eq!(v13.render_body_size(), "    client_max_body_size 1k;\n");

        // HSTS off removes the header line entirely.
        let no_hsts = TransportPolicy::new(
            false,
            TlsVersion::V1_2,
            None,
            CompressionPolicy::Off,
            ByteSize::new(1024).unwrap(),
        )
        .unwrap();
        assert_eq!(no_hsts.render_hsts(), "");
    }
}

#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;

    use super::*;

    fn legacy_protocol() -> impl Strategy<Value = &'static str> {
        prop::sample::select(vec!["TLSv1", "TLSv1.0", "TLSv1.1"])
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_rendered_output_respects_floor_and_cap(
            http3 in any::<bool>(),
            floor in prop::sample::select(vec![TlsVersion::V1_2, TlsVersion::V1_3]),
            rogue in legacy_protocol(),
            hsts_max_age in 0u32..=80_000_000,
            include_subdomains in any::<bool>(),
            preload in any::<bool>(),
            level in 1u8..=9,
            gzip in any::<bool>(),
            cap_mibs in 1u64..=10_000,
        ) {
            let hsts = if preload && hsts_max_age < 31_536_000 {
                None
            } else {
                Some(HstsPolicy::new(hsts_max_age, include_subdomains, preload).unwrap())
            };
            let compression = if gzip {
                CompressionPolicy::Gzip(level)
            } else {
                CompressionPolicy::Brotli(level)
            };
            let policy = TransportPolicy::new(
                http3,
                floor,
                hsts,
                compression,
                ByteSize::new(cap_mibs * 1024 * 1024).unwrap(),
            )
            .unwrap();

            let protocols = policy.render_ssl_protocols();
            // The advertised list never dips below the configured floor.
            let order = ["TLSv1", "TLSv1.0", "TLSv1.1", "TLSv1.2", "TLSv1.3"];
            let floor_rank = match floor {
                TlsVersion::V1_2 => 3,
                TlsVersion::V1_3 => 4,
            };
            let tokens: Vec<&str> = protocols.split_whitespace().collect();
            for version in order.iter().take(floor_rank) {
                prop_assert!(!tokens.contains(version));
            }
            prop_assert!(tokens.contains(&floor.as_str()));

            // The body-size line always matches the cap.
            let expected = format!(
                "    client_max_body_size {};\n",
                policy.body_size_cap().render()
            );
            prop_assert_eq!(policy.render_body_size(), expected);
            prop_assert_eq!(
                policy.body_size_cap().render(),
                format!("{cap_mibs}M")
            );
            // Legacy protocol tokens never appear in the output.
            prop_assert!(!protocols.split_whitespace().any(|t| t == rogue));
        }
    }
}
