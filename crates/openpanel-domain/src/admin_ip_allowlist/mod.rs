//! Admin IP allowlist bounded context: `AdminIpAllowlist`,
//! `AllowlistMode`, `IpCidr`, and the `AllowlistOverride` per-role
//! bypass policy that the `IpAllowlistMiddleware` consumes.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identity::role::Role;

/// Allowlist enforcement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowlistMode {
    /// When the allowlist is empty, the panel is open; otherwise
    /// only entries on the allowlist may authenticate.
    AllowlistOrOpen,
    /// When the allowlist is empty, every peer is denied; only
    /// entries on the allowlist may authenticate.
    AllowlistStrict,
}

/// Per-role override: `Inherit` applies the panel-wide mode;
/// `Bypass` skips the middleware for the role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowlistOverride {
    Inherit,
    Bypass,
}

/// A typed CIDR. IPv4 and IPv6 are supported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpCidr {
    pub address: IpAddr,
    pub prefix_len: u8,
    pub label: Option<String>,
}

impl IpCidr {
    /// Build a new CIDR. The prefix length is bound to the
    /// address family (0..=32 for IPv4, 0..=128 for IPv6).
    pub fn new(
        address: IpAddr,
        prefix_len: u8,
        label: Option<String>,
    ) -> Result<Self, AdminIpAllowlistError> {
        let max = match address {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix_len > max {
            return Err(AdminIpAllowlistError::InvalidPrefixLen {
                address,
                prefix_len,
                max,
            });
        }
        Ok(Self {
            address,
            prefix_len,
            label,
        })
    }
    /// Whether `ip` falls within this CIDR.
    pub fn matches(&self, ip: IpAddr) -> bool {
        match (self.address, ip) {
            (IpAddr::V4(net), IpAddr::V4(addr)) => {
                let mask = prefix_v4(self.prefix_len);
                ipv4_to_u32(net) & mask == ipv4_to_u32(addr) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(addr)) => {
                let mask = prefix_v6(self.prefix_len);
                let (net_h, net_l) = ipv6_to_u128(net);
                let (addr_h, addr_l) = ipv6_to_u128(addr);
                (net_h & mask.0, net_l & mask.1) == (addr_h & mask.0, addr_l & mask.1)
            }
            _ => false,
        }
    }
}

fn prefix_v4(prefix_len: u8) -> u32 {
    if prefix_len == 0 {
        0
    } else if prefix_len >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix_len)
    }
}

fn ipv4_to_u32(addr: Ipv4Addr) -> u32 {
    u32::from(addr.octets()[0]) << 24
        | u32::from(addr.octets()[1]) << 16
        | u32::from(addr.octets()[2]) << 8
        | u32::from(addr.octets()[3])
}

fn prefix_v6(prefix_len: u8) -> (u128, u128) {
    let bits = prefix_len as u32;
    if bits == 0 {
        (0, 0)
    } else if bits >= 128 {
        (u128::MAX, u128::MAX)
    } else if bits > 64 {
        // Mask sits in the upper 64 bits (some bytes of the high
        // half still need to be masked, but the low 64 bits are
        // untouched).
        let high = u128::MAX << (128 - bits);
        (high, u128::MAX)
    } else {
        // Mask sits in the lower 64 bits — i.e. the upper `bits`
        // bits of the high 64-bit portion. Apply the mask to the
        // low part of the (high, low) pair via shift.
        let high = u128::MAX << (64 - bits);
        (high, 0)
    }
}

fn ipv6_to_u128(addr: Ipv6Addr) -> (u128, u128) {
    let octets = addr.octets();
    let mut high: u128 = 0;
    let mut low: u128 = 0;
    for (i, b) in octets.iter().take(8).enumerate() {
        high |= (u128::from(*b)) << (8 * (7 - i));
    }
    for (i, b) in octets.iter().skip(8).enumerate() {
        low |= (u128::from(*b)) << (8 * (7 - i));
    }
    (high, low)
}

/// The admin IP allowlist aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminIpAllowlist {
    pub mode: AllowlistMode,
    pub entries: Vec<IpCidr>,
    pub role_overrides: BTreeMap<Role, AllowlistOverride>,
    pub updated_at: DateTime<Utc>,
}

impl AdminIpAllowlist {
    /// Build a new allowlist.
    pub fn new(
        mode: AllowlistMode,
        entries: Vec<IpCidr>,
        role_overrides: BTreeMap<Role, AllowlistOverride>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            mode,
            entries,
            role_overrides,
            updated_at: now,
        }
    }

    /// Match an IP against the allowlist. Returns the matched
    /// CIDR label if found, or `None` if the IP is not on the
    /// allowlist.
    pub fn find_match(&self, ip: IpAddr) -> Option<&IpCidr> {
        self.entries.iter().find(|c| c.matches(ip))
    }

    /// Whether `ip` is allowed under the current allowlist.
    /// Empty allowlist + `AllowlistOrOpen` returns `true`.
    /// Empty allowlist + `AllowlistStrict` returns `false`.
    pub fn allows(&self, ip: IpAddr, role: Option<Role>) -> bool {
        if let Some(r) = role
            && let Some(AllowlistOverride::Bypass) = self.role_overrides.get(&r)
        {
            return true;
        }
        match self.mode {
            AllowlistMode::AllowlistOrOpen if self.entries.is_empty() => true,
            AllowlistMode::AllowlistOrOpen => self.find_match(ip).is_some(),
            AllowlistMode::AllowlistStrict => self.find_match(ip).is_some(),
        }
    }
}

/// Errors that can occur in the admin IP allowlist bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AdminIpAllowlistError {
    /// The prefix length is out of range for the address family.
    #[error("invalid prefix length {prefix_len} for {address} (max {max})")]
    InvalidPrefixLen {
        address: IpAddr,
        prefix_len: u8,
        max: u8,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_rejects_invalid_prefix_len() {
        let err = IpCidr::new(
            "10.0.0.0".parse::<IpAddr>().unwrap(),
            33,
            None,
        )
        .expect_err("must reject");
        assert!(matches!(err, AdminIpAllowlistError::InvalidPrefixLen { .. }));
    }

    #[test]
    fn cidr_matches_ipv4() {
        let cidr = IpCidr::new(
            "10.0.0.0".parse::<IpAddr>().unwrap(),
            8,
            Some("office".to_string()),
        )
        .unwrap();
        assert!(cidr.matches("10.0.0.5".parse::<IpAddr>().unwrap()));
        assert!(!cidr.matches("11.0.0.5".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn cidr_matches_ipv6() {
        let cidr = IpCidr::new(
            "2001:db8::".parse::<IpAddr>().unwrap(),
            32,
            Some("office".to_string()),
        )
        .unwrap();
        assert!(cidr.matches("2001:db8::1".parse::<IpAddr>().unwrap()));
        assert!(!cidr.matches("2001:db9::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn or_open_allows_when_empty() {
        let allowlist = AdminIpAllowlist::new(
            AllowlistMode::AllowlistOrOpen,
            Vec::new(),
            BTreeMap::new(),
            Utc::now(),
        );
        assert!(allowlist.allows("203.0.113.1".parse::<IpAddr>().unwrap(), None));
    }

    #[test]
    fn strict_denies_when_empty() {
        let allowlist = AdminIpAllowlist::new(
            AllowlistMode::AllowlistStrict,
            Vec::new(),
            BTreeMap::new(),
            Utc::now(),
        );
        assert!(!allowlist.allows("203.0.113.1".parse::<IpAddr>().unwrap(), None));
    }

    #[test]
    fn role_override_bypass() {
        let mut overrides = BTreeMap::new();
        overrides.insert(Role::Owner, AllowlistOverride::Bypass);
        let allowlist = AdminIpAllowlist::new(
            AllowlistMode::AllowlistStrict,
            Vec::new(),
            overrides,
            Utc::now(),
        );
        assert!(allowlist.allows("203.0.113.1".parse::<IpAddr>().unwrap(), Some(Role::Owner)));
        assert!(!allowlist.allows("203.0.113.1".parse::<IpAddr>().unwrap(), Some(Role::User)));
    }

    #[test]
    fn find_match_returns_label() {
        let entries = vec![IpCidr::new(
            "10.0.0.0".parse::<IpAddr>().unwrap(),
            8,
            Some("office".to_string()),
        )
        .unwrap()];
        let allowlist = AdminIpAllowlist::new(
            AllowlistMode::AllowlistStrict,
            entries,
            BTreeMap::new(),
            Utc::now(),
        );
        let match_ = allowlist.find_match("10.0.0.5".parse::<IpAddr>().unwrap());
        assert_eq!(match_.unwrap().label.as_deref(), Some("office"));
    }
}
