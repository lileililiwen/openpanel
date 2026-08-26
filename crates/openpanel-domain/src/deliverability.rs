//! Email deliverability monitoring domain: DNSBL query-name
//! construction and listing state. Pure; DNS resolution is injected
//! via a port by the application layer.

use std::net::IpAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One configured DNS blocklist zone (e.g. `zen.spamhaus.org`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlocklistZone(pub String);

impl BlocklistZone {
    /// Construct with basic sanity: non-empty, no scheme, no slash.
    pub fn new(zone: impl Into<String>) -> Option<Self> {
        let zone = zone.into();
        if zone.is_empty() || zone.contains("://") || zone.contains('/') || !zone.contains('.') {
            return None;
        }
        Some(Self(zone))
    }
}

/// Build the DNSBL lookup name for `ip` against `zone`: IPv4 octets
/// reversed; IPv6 expanded to nibbles reversed.
pub fn dnsbl_query_name(ip: IpAddr, zone: &BlocklistZone) -> String {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("{}.{}.{}.{}.{}", o[3], o[2], o[1], o[0], zone.0)
        }
        IpAddr::V6(v6) => {
            let segs = v6.segments();
            let mut out = String::new();
            // Walk segments from least significant to most, nibbles
            // from low to high: this yields the reversed-nibble form
            // DNSBL zones expect.
            for seg in segs.iter().rev() {
                for shift in [0u16, 4, 8, 12] {
                    out.push_str(&format!("{:x}.", (seg >> shift) & 0xF));
                }
            }
            out.push_str(&zone.0);
            out
        }
    }
}

/// Listing state for one (ip, zone) pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    ip: IpAddr,
    zone: String,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}

impl Listing {
    /// A newly observed listing.
    pub fn listed(ip: IpAddr, zone: &str, now: DateTime<Utc>) -> Self {
        Self {
            ip,
            zone: zone.to_string(),
            first_seen: now,
            last_seen: now,
            resolved_at: None,
        }
    }

    /// Upsert semantics: a repeat listing keeps `first_seen` and
    /// refreshes `last_seen`; a clear stamps `resolved_at`.
    pub fn upsert(&mut self, listed_now: bool, now: DateTime<Utc>) {
        self.last_seen = now;
        if listed_now {
            self.resolved_at = None;
        } else if self.resolved_at.is_none() {
            self.resolved_at = Some(now);
        }
    }

    /// The listed address.
    pub fn ip(&self) -> IpAddr {
        self.ip
    }

    /// The zone.
    pub fn zone(&self) -> &str {
        &self.zone
    }

    /// First observation.
    pub fn first_seen(&self) -> DateTime<Utc> {
        self.first_seen
    }

    /// Last observation.
    pub fn last_seen(&self) -> DateTime<Utc> {
        self.last_seen
    }

    /// When the listing cleared, if it did.
    pub fn resolved_at(&self) -> Option<DateTime<Utc>> {
        self.resolved_at
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[test]
    fn test_dnsbl_query_name_construction() {
        let zone = BlocklistZone::new("zen.spamhaus.org").unwrap();
        // IPv4 192.0.2.5 → 5.2.0.192.zen.spamhaus.org.
        assert_eq!(
            dnsbl_query_name(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 5)), &zone),
            "5.2.0.192.zen.spamhaus.org"
        );
        // IPv6 nibble expansion verified: 2001:db8::1 → reversed
        // nibbles of the full 2001:0db8:0000:...:0001 form.
        let v6: IpAddr = "2001:db8::1".parse().unwrap();
        let q = dnsbl_query_name(v6, &zone);
        assert!(q.ends_with(".zen.spamhaus.org"));
        // Full expansion is 32 nibbles before the zone.
        let ip_labels = q
            .strip_suffix(&format!(".{}", zone.0))
            .expect("zone suffix")
            .split('.')
            .count();
        assert_eq!(ip_labels, 32);
        // First label is the last nibble of the address (1).
        assert!(q.starts_with("1.0."));
        // Zone sanity.
        assert!(BlocklistZone::new("http://evil").is_none());
        assert!(BlocklistZone::new("nosuffix").is_none());
    }

    #[test]
    fn test_listing_upsert_preserves_first_seen_and_marks_resolved() {
        let t0 = Utc::now();
        let mut listing = Listing::listed(
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 5)),
            "zen.spamhaus.org",
            t0,
        );
        assert!(listing.resolved_at().is_none());

        // Repeat listing keeps first_seen, refreshes last_seen.
        let t1 = t0 + chrono::Duration::days(1);
        listing.upsert(true, t1);
        assert_eq!(listing.first_seen(), t0);
        assert_eq!(listing.last_seen(), t1);
        assert!(listing.resolved_at().is_none());

        // Clear stamps resolved_at once.
        let t2 = t0 + chrono::Duration::days(2);
        listing.upsert(false, t2);
        assert_eq!(listing.resolved_at(), Some(t2));

        // A later clear does not move resolved_at.
        let t3 = t0 + chrono::Duration::days(3);
        listing.upsert(false, t3);
        assert_eq!(listing.resolved_at(), Some(t2));
    }
}

/// Per-source aggregate statistics from one DMARC report row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmarcSourceStat {
    source_ip: IpAddr,
    messages: u64,
    dkim_pass: u64,
    spf_pass: u64,
}

impl DmarcSourceStat {
    /// The source address.
    pub fn source_ip(&self) -> IpAddr {
        self.source_ip
    }

    /// Message count for the day.
    pub fn messages(&self) -> u64 {
        self.messages
    }

    /// Messages passing DKIM.
    pub fn dkim_pass(&self) -> u64 {
        self.dkim_pass
    }

    /// Messages passing SPF.
    pub fn spf_pass(&self) -> u64 {
        self.spf_pass
    }
}

/// DMARC report parse failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeliverabilityError {
    /// The report exceeds the 10 MiB input cap.
    #[error("report exceeds the 10 MiB cap")]
    ReportTooLarge,
    /// The report carries a DOCTYPE or entity declaration.
    #[error("report contains DOCTYPE/entities")]
    ReportDoctype,
    /// The report is not parseable aggregate-report XML.
    #[error("report malformed")]
    ReportMalformed,
}

/// Input cap for uploaded reports.
pub const REPORT_MAX_BYTES: usize = 10 * 1024 * 1024;

/// Extract the text of the first `<tag>…</tag>` within `xml`.
fn first_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

/// Parse a DMARC aggregate report into per-source stats. Pure and
/// allocation-bounded: rejects oversized input, DOCTYPE/entity
/// declarations (XXE class), and malformed rows without panicking.
pub fn parse_dmarc_report(xml: &str) -> Result<Vec<DmarcSourceStat>, DeliverabilityError> {
    if xml.len() > REPORT_MAX_BYTES {
        return Err(DeliverabilityError::ReportTooLarge);
    }
    let lower = xml.to_ascii_lowercase();
    if lower.contains("<!doctype") || lower.contains("<!entity") {
        return Err(DeliverabilityError::ReportDoctype);
    }
    let mut stats = Vec::new();
    for record in xml.split("<record>").skip(1) {
        let record = match record.split("</record>").next() {
            Some(r) => r,
            None => return Err(DeliverabilityError::ReportMalformed),
        };
        let source_ip: IpAddr = first_tag(record, "source_ip")
            .ok_or(DeliverabilityError::ReportMalformed)?
            .trim()
            .parse()
            .map_err(|_| DeliverabilityError::ReportMalformed)?;
        let count: u64 = first_tag(record, "count")
            .ok_or(DeliverabilityError::ReportMalformed)?
            .trim()
            .parse()
            .map_err(|_| DeliverabilityError::ReportMalformed)?;
        let pass = |kind: &str| -> Result<u64, DeliverabilityError> {
            match first_tag(record, kind).as_deref() {
                Some(v) if v.trim() == "pass" => Ok(count),
                Some(_) => Ok(0),
                None => Err(DeliverabilityError::ReportMalformed),
            }
        };
        stats.push(DmarcSourceStat {
            source_ip,
            messages: count,
            dkim_pass: pass("dkim")?,
            spf_pass: pass("spf")?,
        });
    }
    if stats.is_empty() {
        // A report with no records section is malformed by definition.
        if !xml.contains("<record>") {
            return Err(DeliverabilityError::ReportMalformed);
        }
    }
    Ok(stats)
}

/// Stable drift codes emitted by the auth audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftCode {
    /// SPF record lacks an `all` mechanism.
    SpfMissingAll,
    /// DKIM selector key hash does not match the managed key.
    DkimSelectorMismatch,
    /// DMARC record has no `rua=` reporting address.
    DmarcRuaAbsent,
}

impl DriftCode {
    /// Stable wire code.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SpfMissingAll => "spf_missing_all",
            Self::DkimSelectorMismatch => "dkim_selector_mismatch",
            Self::DmarcRuaAbsent => "dmarc_rua_absent",
        }
    }
}

/// Compare live DNS state against managed values and produce exactly
/// one drift entry per detected problem.
pub fn auth_audit(
    spf_record: Option<&str>,
    dkim_key_matches: bool,
    dmarc_record: Option<&str>,
) -> Vec<DriftCode> {
    let mut drift = Vec::new();
    match spf_record {
        Some(record) if !record.contains("all") => {
            drift.push(DriftCode::SpfMissingAll);
        }
        None => drift.push(DriftCode::SpfMissingAll),
        _ => {}
    }
    if !dkim_key_matches {
        drift.push(DriftCode::DkimSelectorMismatch);
    }
    match dmarc_record {
        Some(record) if !record.contains("rua=") => {
            drift.push(DriftCode::DmarcRuaAbsent);
        }
        None => drift.push(DriftCode::DmarcRuaAbsent),
        _ => {}
    }
    drift
}

#[cfg(test)]
mod report_tests {
    use super::*;

    const FIXTURE_TWO_ROWS: &str = r#"<?xml version="1.0"?>
<feedback>
  <record>
    <row><source_ip>203.0.113.7</source_ip><count>12</count>
      <policy_evaluated><dkim>pass</dkim><spf>fail</spf></policy_evaluated></row>
  </record>
  <record>
    <row><source_ip>198.51.100.9</source_ip><count>3</count>
      <policy_evaluated><dkim>fail</dkim><spf>pass</spf></policy_evaluated></row>
  </record>
</feedback>"#;

    #[test]
    fn test_dmarc_parser_two_rows_and_rejections() {
        let stats = parse_dmarc_report(FIXTURE_TWO_ROWS).unwrap();
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].source_ip().to_string(), "203.0.113.7");
        assert_eq!(stats[0].messages(), 12);
        assert_eq!(stats[0].dkim_pass(), 12);
        assert_eq!(stats[0].spf_pass(), 0);
        assert_eq!(stats[1].messages(), 3);
        assert_eq!(stats[1].dkim_pass(), 0);
        assert_eq!(stats[1].spf_pass(), 3);

        // Oversize input rejected before any parsing.
        let big = format!("<feedback>{}</feedback>", "x".repeat(REPORT_MAX_BYTES + 1));
        assert_eq!(
            parse_dmarc_report(&big).unwrap_err(),
            DeliverabilityError::ReportTooLarge
        );

        // DOCTYPE / entity-bearing input rejected before parse.
        assert_eq!(
            parse_dmarc_report(
                "<!DOCTYPE feedback [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]><feedback/>"
            )
            .unwrap_err(),
            DeliverabilityError::ReportDoctype
        );
        assert_eq!(
            parse_dmarc_report("<!ENTITY evil 'x'><feedback/>").unwrap_err(),
            DeliverabilityError::ReportDoctype
        );

        // Malformed XML → ReportMalformed, never a panic.
        assert_eq!(
            parse_dmarc_report("<feedback><record><row></feedback>").unwrap_err(),
            DeliverabilityError::ReportMalformed
        );
        assert_eq!(
            parse_dmarc_report("not xml at all").unwrap_err(),
            DeliverabilityError::ReportMalformed
        );
    }

    #[test]
    fn test_auth_audit_drift_codes_are_stable_and_single() {
        // All healthy → no drift.
        assert!(
            auth_audit(
                Some("v=spf1 include:_spf.example.com -all"),
                true,
                Some("v=DMARC1; rua=mailto:dmarc@example.com")
            )
            .is_empty()
        );

        // Each problem yields exactly one stable entry.
        assert_eq!(
            auth_audit(
                Some("v=spf1 include:_spf.example.com"),
                true,
                Some("v=DMARC1; rua=mailto:d@e.com")
            ),
            vec![DriftCode::SpfMissingAll]
        );
        assert_eq!(
            auth_audit(
                Some("v=spf1 -all"),
                false,
                Some("v=DMARC1; rua=mailto:d@e.com")
            ),
            vec![DriftCode::DkimSelectorMismatch]
        );
        assert_eq!(
            auth_audit(Some("v=spf1 -all"), true, Some("v=DMARC1; p=reject")),
            vec![DriftCode::DmarcRuaAbsent]
        );
        // Missing records count as drift too.
        assert_eq!(
            auth_audit(None, true, None),
            vec![DriftCode::SpfMissingAll, DriftCode::DmarcRuaAbsent]
        );
        // Wire codes are stable strings.
        assert_eq!(DriftCode::SpfMissingAll.as_str(), "spf_missing_all");
        assert_eq!(
            DriftCode::DkimSelectorMismatch.as_str(),
            "dkim_selector_mismatch"
        );
        assert_eq!(DriftCode::DmarcRuaAbsent.as_str(), "dmarc_rua_absent");
    }
}

#[cfg(test)]
mod report_prop_tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_report_rows_sum_to_source_stats(
            rows in proptest::collection::vec(
                (
                    (1u8..=223, 0u8..=255, 0u8..=255, 0u8..=255),
                    1u64..=500,
                    any::<bool>(),
                    any::<bool>(),
                ),
                1..12,
            ),
        ) {
            let mut xml = String::from("<feedback>");
            let mut expected_total = 0u64;
            for ((a, b, c, d), count, dkim_pass, spf_pass) in &rows {
                expected_total += count;
                let dkim_s = if *dkim_pass { "pass" } else { "fail" };
                let spf_s = if *spf_pass { "pass" } else { "fail" };
                xml.push_str(&format!(
                    "<record><row><source_ip>{a}.{b}.{c}.{d}</source_ip><count>{count}</count>\
                     <policy_evaluated><dkim>{dkim_s}</dkim><spf>{spf_s}</spf></policy_evaluated></row></record>",
                ));
            }
            xml.push_str("</feedback>");

            let stats = parse_dmarc_report(&xml).unwrap();
            // One stat per row; message counts sum exactly.
            prop_assert_eq!(stats.len(), rows.len());
            let total: u64 = stats.iter().map(|s| s.messages()).sum();
            prop_assert_eq!(total, expected_total);
            // Per-row fidelity.
            for (stat, (_, count, dkim_pass, spf_pass)) in stats.iter().zip(rows.iter()) {
                prop_assert_eq!(stat.messages(), *count);
                prop_assert_eq!(stat.dkim_pass(), if *dkim_pass { *count } else { 0 });
                prop_assert_eq!(stat.spf_pass(), if *spf_pass { *count } else { 0 });
            }
            // The stat struct retains only ip/counters — no raw
            // identifiers beyond the source IP.
            let serialized = serde_json::to_string(&stats).unwrap();
            prop_assert!(!serialized.contains("record"));
            prop_assert!(!serialized.contains("policy_evaluated"));
        }
    }
}

/// DNS resolution port. The application layer implements it over a
/// real resolver; tests inject fakes.
#[async_trait::async_trait]
pub trait ResolverPort: Send + Sync + 'static {
    /// TXT records for `name` (used by auth audits).
    async fn txt(&self, name: &str) -> Result<Vec<String>, String>;
    /// A/AAAA records for `name`.
    async fn resolve_a(&self, name: &str) -> Result<Vec<IpAddr>, String>;
}

/// Query `zone` for `ip`: a non-empty A-record answer means listed.
pub async fn is_listed(
    resolver: &dyn ResolverPort,
    ip: IpAddr,
    zone: &BlocklistZone,
) -> Result<bool, String> {
    let name = dnsbl_query_name(ip, zone);
    Ok(!resolver.resolve_a(&name).await?.is_empty())
}

#[cfg(test)]
mod resolver_tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use std::{collections::HashMap, net::Ipv4Addr, sync::Mutex};

    use super::*;

    struct FakeResolver {
        answers: Mutex<HashMap<String, Vec<IpAddr>>>,
    }

    impl FakeResolver {
        fn with_a(name: &str, ip: IpAddr) -> Self {
            let mut m = HashMap::new();
            m.insert(name.to_string(), vec![ip]);
            Self {
                answers: Mutex::new(m),
            }
        }
    }

    #[async_trait::async_trait]
    impl ResolverPort for FakeResolver {
        async fn txt(&self, _name: &str) -> Result<Vec<String>, String> {
            Ok(vec![])
        }

        async fn resolve_a(&self, name: &str) -> Result<Vec<IpAddr>, String> {
            Ok(self
                .answers
                .lock()
                .unwrap()
                .get(name)
                .cloned()
                .unwrap_or_default())
        }
    }

    #[test]
    fn is_listed_uses_constructed_query_and_a_record_presence() {
        let zone = BlocklistZone::new("zen.spamhaus.org").unwrap();
        let ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 5));
        let query = "5.2.0.192.zen.spamhaus.org";

        // Listed: fake answers the constructed name.
        let listed_resolver = FakeResolver::with_a(query, "127.0.0.2".parse().unwrap());
        assert!(pollster::block_on(is_listed(&listed_resolver, ip, &zone)).unwrap());

        // Clear: empty answer.
        let clean_resolver =
            FakeResolver::with_a("unrelated.example", "127.0.0.1".parse().unwrap());
        assert!(!pollster::block_on(is_listed(&clean_resolver, ip, &zone)).unwrap());
    }
}
