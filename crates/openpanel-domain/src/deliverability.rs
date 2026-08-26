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
