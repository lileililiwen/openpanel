//! Pure MySQL host-pattern derivation from CIDR allow-list entries
//! and the reconcile diff between desired and current grant hosts.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use super::DbPrivilegeError;

/// A validated MySQL host pattern derived from one CIDR entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MysqlHostPattern(String);

impl MysqlHostPattern {
    /// The pattern string (e.g. `203.0.113.7` or `203.0.113.%`).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Derive the MySQL host pattern for one CIDR. Prefixes below /16
/// (IPv4) or /48 (IPv6) are too broad and rejected unless a global
/// opt-in is passed.
pub fn mysql_host_pattern(
    cidr: &str,
    global_opt_in: bool,
) -> Result<MysqlHostPattern, DbPrivilegeError> {
    let (addr, prefix) = parse_cidr(cidr)?;
    match addr {
        IpAddr::V4(v4) => {
            if prefix < 16 && !global_opt_in {
                return Err(DbPrivilegeError::PrefixTooBroad);
            }
            let octets = v4.octets();
            let pattern = if prefix >= 32 {
                format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
            } else if prefix >= 24 {
                format!("{}.{}.{}.%", octets[0], octets[1], octets[2])
            } else if prefix >= 16 {
                format!("{}.{}.%", octets[0], octets[1])
            } else {
                // Only reachable with the opt-in.
                format!("{}.%.%.%", octets[0])
            };
            Ok(MysqlHostPattern(pattern))
        }
        IpAddr::V6(v6) => {
            if prefix < 48 && !global_opt_in {
                return Err(DbPrivilegeError::PrefixTooBroad);
            }
            let segments = v6.segments();
            let full: Vec<String> = segments.iter().map(|s| format!("{s:x}")).collect();
            let kept = (prefix as usize).div_ceil(16).min(8);
            let mut parts = full[..kept].to_vec();
            if kept < 8 {
                parts.push("%".to_string());
            }
            Ok(MysqlHostPattern(parts.join(":")))
        }
    }
}

fn parse_cidr(cidr: &str) -> Result<(IpAddr, u8), DbPrivilegeError> {
    let (addr_part, prefix_part) = cidr
        .split_once('/')
        .ok_or_else(|| DbPrivilegeError::InvalidPolicy("CIDR lacks /prefix".into()))?;
    let addr: IpAddr = addr_part
        .parse()
        .map_err(|_| DbPrivilegeError::InvalidPolicy(format!("bad CIDR address `{addr_part}`")))?;
    let prefix: u8 = prefix_part
        .parse()
        .map_err(|_| DbPrivilegeError::InvalidPolicy(format!("bad prefix `{prefix_part}`")))?;
    let max = match addr {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    };
    if prefix > max {
        return Err(DbPrivilegeError::InvalidPolicy(format!(
            "prefix {prefix} exceeds /{max}"
        )));
    }
    Ok((addr, prefix))
}

/// The desired grant-host set for an ACL: always `localhost` plus one
/// derived pattern per CIDR.
pub fn desired_hosts(
    allow_cidrs: &[String],
    global_opt_in: bool,
) -> Result<Vec<String>, DbPrivilegeError> {
    let mut hosts = vec!["localhost".to_string()];
    for cidr in allow_cidrs {
        let pattern = mysql_host_pattern(cidr, global_opt_in)?;
        if !hosts.iter().any(|h| h == pattern.as_str()) {
            hosts.push(pattern.as_str().to_string());
        }
    }
    Ok(hosts)
}

/// One reconcile step against the live grant table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileStep {
    /// Create `user@host`.
    Create {
        /// The derived host pattern.
        host: String,
    },
    /// Drop `user@host`.
    Drop {
        /// The derived host pattern.
        host: String,
    },
}

/// Diff desired vs current host set into ordered steps: creates
/// first, then drops.
pub fn reconcile_diff(desired: &[String], current: &[String]) -> Vec<ReconcileStep> {
    let mut steps = Vec::new();
    for host in desired {
        if !current.contains(host) {
            steps.push(ReconcileStep::Create { host: host.clone() });
        }
    }
    for host in current {
        if !desired.contains(host) {
            steps.push(ReconcileStep::Drop { host: host.clone() });
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysql_host_pattern_derivations() {
        // /32 → exact host.
        assert_eq!(
            mysql_host_pattern("203.0.113.7/32", false)
                .unwrap()
                .as_str(),
            "203.0.113.7"
        );
        // /24 → a.b.c.%.
        assert_eq!(
            mysql_host_pattern("203.0.113.0/24", false)
                .unwrap()
                .as_str(),
            "203.0.113.%"
        );
        // IPv6 /64 → wildcard form.
        assert_eq!(
            mysql_host_pattern("2001:db8:0:0:0:0:0:1/64", false)
                .unwrap()
                .as_str(),
            "2001:db8:0:0:%"
        );
        // Prefix below the floor is rejected without opt-in…
        assert_eq!(
            mysql_host_pattern("10.0.0.0/8", false).unwrap_err(),
            DbPrivilegeError::PrefixTooBroad
        );
        assert_eq!(
            mysql_host_pattern("2001:db8::/32", false).unwrap_err(),
            DbPrivilegeError::PrefixTooBroad
        );
        // …and allowed with it.
        assert_eq!(
            mysql_host_pattern("10.0.0.0/8", true).unwrap().as_str(),
            "10.%.%.%"
        );
        // Malformed input.
        assert!(mysql_host_pattern("not-a-cidr", false).is_err());
        assert!(mysql_host_pattern("203.0.113.7", false).is_err());
        assert!(mysql_host_pattern("203.0.113.7/33", false).is_err());
    }

    #[test]
    fn test_reconcile_diff_and_desired_hosts() {
        // Desired {localhost, p1} vs current {localhost, p1, p2}
        // yields exactly one DROP for p2.
        let steps = reconcile_diff(
            &["localhost".into(), "203.0.113.%".into()],
            &[
                "localhost".into(),
                "203.0.113.%".into(),
                "198.51.100.%".into(),
            ],
        );
        assert_eq!(
            steps,
            vec![ReconcileStep::Drop {
                host: "198.51.100.%".into()
            }]
        );

        // Empty ACL → localhost only.
        let desired = desired_hosts(&[], false).unwrap();
        assert_eq!(desired, vec!["localhost".to_string()]);

        // Creates come before drops.
        let steps = reconcile_diff(
            &["localhost".into(), "203.0.113.%".into()],
            &["localhost".into(), "198.51.100.%".into()],
        );
        assert_eq!(
            steps,
            vec![
                ReconcileStep::Create {
                    host: "203.0.113.%".into()
                },
                ReconcileStep::Drop {
                    host: "198.51.100.%".into()
                },
            ]
        );
    }
}

#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_patterns_unique_and_never_bare_wildcard(
            a_octets in (1u8..=223, 0u8..=255, 0u8..=255, 0u8..=255),
            b_octets in (1u8..=223, 0u8..=255, 0u8..=255, 0u8..=255),
            prefix_a in 16u8..=32,
            prefix_b in 16u8..=32,
        ) {
            let cidr_a = format!("{}.{}.{}.{}/{}", a_octets.0, a_octets.1, a_octets.2, a_octets.3, prefix_a);
            let cidr_b = format!("{}.{}.{}.{}/{}", b_octets.0, b_octets.1, b_octets.2, b_octets.3, prefix_b);
            let p_a = mysql_host_pattern(&cidr_a, false).unwrap();
            let p_b = mysql_host_pattern(&cidr_b, false).unwrap();

            // Derived patterns are unique per distinct CIDR unless
            // both coarse prefixes collapse to the same wildcard.
            if !(prefix_a <= 23 && prefix_b <= 23
                && a_octets.0 == b_octets.0 && a_octets.1 == b_octets.1)
            {
                prop_assert_ne!(p_a.as_str(), p_b.as_str());
            }
            // No derived pattern is the bare `%` without opt-in.
            prop_assert_ne!(p_a.as_str().to_string(), "%");
            prop_assert_ne!(p_b.as_str().to_string(), "%");
            // Patterns never contain whitespace or quotes.
            prop_assert!(!p_a.as_str().contains([' ', '\'', '"']));
        }
    }
}
