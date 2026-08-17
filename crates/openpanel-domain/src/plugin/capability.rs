//! Typed capability tokens the panel grants at install time.
//!
//! Each plugin declares the typed capabilities it needs at install
//! time. The supervisor enforces them at every syscall; a plugin
//! cannot escalate at runtime. Tokens are versioned (`v32`) so a
//! new host can introduce a new one without invalidating old
//! manifests.

use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};

/// Capability major version. The framework refuses manifests with
/// a higher version than this.
pub const CAPABILITY_VERSION: u32 = 1;

/// Capability token.
///
/// The discriminant string is a stable, scoped identifier; the
/// framework MUST reject manifests whose tokens carry a higher
/// capability version than the host supports.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    /// Major capability version the plugin was compiled against.
    pub version: u32,
    /// Stable token name, e.g. `"system-services:read"`.
    pub name: String,
}

impl Capability {
    /// Construct a capability at the current version.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: CAPABILITY_VERSION,
            name: name.into(),
        }
    }

    /// Validate the token name: lowercase, dotted, `[a-z0-9-_:]+`.
    pub fn is_valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 128
            && name.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'.' | b'-' | b'_' | b':')
            })
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@v{}", self.name, self.version)
    }
}

/// Owned set of capabilities a plugin declares at install time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilitySet(BTreeSet<Capability>);

impl CapabilitySet {
    /// Empty set.
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// Construct from an iterator of token names.
    pub fn from_names<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self(
            names
                .into_iter()
                .map(|n| Capability::new(n.into()))
                .collect(),
        )
    }

    /// Returns `true` if the set contains `cap`.
    pub fn contains(&self, cap: &Capability) -> bool {
        self.0.contains(cap)
    }

    /// Returns the number of capabilities.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if there are no capabilities.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over the capabilities in stable order.
    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.0.iter()
    }

    /// Validate every capability; returns the first invalid name.
    pub fn validate(&self) -> Result<(), &'static str> {
        for cap in &self.0 {
            if cap.version > CAPABILITY_VERSION {
                return Err("capability version too high");
            }
            if !Capability::is_valid_name(&cap.name) {
                return Err("invalid capability name");
            }
        }
        Ok(())
    }
}

impl FromIterator<Capability> for CapabilitySet {
    fn from_iter<T: IntoIterator<Item = Capability>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_display_round_trip() {
        let cap = Capability::new("system-services:read");
        assert_eq!(format!("{cap}"), "system-services:read@v1");
        assert_eq!(cap, Capability::new("system-services:read"));
    }

    #[test]
    fn capability_set_contains_and_iter() {
        let set: CapabilitySet = [Capability::new("a:read"), Capability::new("b:write")]
            .into_iter()
            .collect();
        assert_eq!(set.len(), 2);
        assert!(set.contains(&Capability::new("a:read")));
        assert!(!set.contains(&Capability::new("a:write")));
        let names: Vec<_> = set.iter().map(|c| c.name.clone()).collect();
        assert_eq!(names, vec!["a:read".to_string(), "b:write".to_string()]);
    }

    #[test]
    fn capability_set_validate_rejects_bad_names() {
        let set = CapabilitySet::new();
        // Push directly through BTreeSet to bypass normal constructor.
        let _ = set;
        let bad = Capability {
            version: 1,
            name: "Bad-Name!".into(),
        };
        let set = CapabilitySet::new();
        let _ = set;
        let set: CapabilitySet = [bad].into_iter().collect();
        assert!(set.validate().is_err());
    }

    #[test]
    fn capability_set_validate_rejects_too_high_version() {
        let set: CapabilitySet = [Capability {
            version: 99,
            name: "x:read".into(),
        }]
        .into_iter()
        .collect();
        assert!(set.validate().is_err());
    }
}
