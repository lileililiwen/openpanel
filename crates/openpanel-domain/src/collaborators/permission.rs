//! Typed permission set for collaborators.
//!
//! Permissions are bit-flagged so grant unions are cheap; the
//! set is intentionally closed (no `Other` variant) so callers can
//! never grant a scope outside the allowlist.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A typed permission scope. Adding a new variant is a breaking
/// change for the `FromStr`/`as_str` contract; open variants MUST
/// not be introduced without a schema bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Read/write file content within the site's chroot.
    File,
    /// Database CRUD on the site's databases.
    Database,
    /// Mail mailbox / alias management.
    Mail,
    /// Cron job creation / run.
    Cron,
}

impl Permission {
    /// Stable string form.
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::File => "file",
            Permission::Database => "database",
            Permission::Mail => "mail",
            Permission::Cron => "cron",
        }
    }

    /// Parse from string. Returns `None` for unrecognised scopes.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "file" => Some(Permission::File),
            "database" => Some(Permission::Database),
            "mail" => Some(Permission::Mail),
            "cron" => Some(Permission::Cron),
            _ => None,
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Bit-flag set of [`Permission`] values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PermissionSet(pub u8);

impl PermissionSet {
    /// Empty set.
    pub const EMPTY: PermissionSet = PermissionSet(0);

    /// Construct from a single permission.
    pub fn single(p: Permission) -> Self {
        Self(1 << p.bit())
    }

    /// Insert a permission.
    pub fn insert(&mut self, p: Permission) {
        self.0 |= 1 << p.bit();
    }

    /// Remove a permission.
    pub fn remove(&mut self, p: Permission) {
        self.0 &= !(1 << p.bit());
    }

    /// Returns `true` if `p` is set.
    pub fn contains(&self, p: Permission) -> bool {
        self.0 & (1 << p.bit()) != 0
    }

    /// Returns `true` if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Returns the number of distinct permissions set.
    pub fn len(&self) -> u32 {
        self.0.count_ones()
    }

    /// Returns the union of `self` and `other`.
    pub fn union(self, other: PermissionSet) -> PermissionSet {
        PermissionSet(self.0 | other.0)
    }

    /// Returns the intersection of `self` and `other`.
    pub fn intersection(self, other: PermissionSet) -> PermissionSet {
        PermissionSet(self.0 & other.0)
    }

    /// Iterate over the contained permissions in stable order.
    pub fn iter(self) -> impl Iterator<Item = Permission> {
        let mut bits = self.0;
        std::iter::from_fn(move || {
            if bits == 0 {
                None
            } else {
                let idx = bits.trailing_zeros() as u8;
                bits &= bits - 1;
                Some(permission_from_bit(idx))
            }
        })
    }
}

impl FromIterator<Permission> for PermissionSet {
    /// Build a set from an iterator of permissions.
    fn from_iter<I: IntoIterator<Item = Permission>>(iter: I) -> Self {
        let mut out = Self::EMPTY;
        for p in iter {
            out.insert(p);
        }
        out
    }
}

impl Permission {
    fn bit(self) -> u8 {
        match self {
            Permission::File => 0,
            Permission::Database => 1,
            Permission::Mail => 2,
            Permission::Cron => 3,
        }
    }
}

fn permission_from_bit(bit: u8) -> Permission {
    match bit {
        0 => Permission::File,
        1 => Permission::Database,
        2 => Permission::Mail,
        3 => Permission::Cron,
        // Fall back to File for unknown bits; the set is internally
        // restricted so this path is unreachable.
        _ => Permission::File,
    }
}

impl std::str::FromStr for Permission {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Permission::parse(s).ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_round_trips() {
        for p in [
            Permission::File,
            Permission::Database,
            Permission::Mail,
            Permission::Cron,
        ] {
            let s = p.as_str();
            let parsed: Permission = s.parse().unwrap();
            assert_eq!(parsed, p);
        }
        assert!(Permission::parse("bogus").is_none());
    }

    #[test]
    fn permission_set_union_and_iter() {
        let a = PermissionSet::from_iter([Permission::File, Permission::Database]);
        let b = PermissionSet::from_iter([Permission::Database, Permission::Mail]);
        let union = a.union(b);
        assert_eq!(union.len(), 3);
        assert!(union.contains(Permission::File));
        assert!(union.contains(Permission::Database));
        assert!(union.contains(Permission::Mail));
        assert!(!union.contains(Permission::Cron));

        let inter = a.intersection(b);
        assert!(inter.contains(Permission::Database));
        assert!(!inter.contains(Permission::File));
        assert!(!inter.contains(Permission::Mail));
    }

    #[test]
    fn empty_set_has_no_bits() {
        let empty = PermissionSet::EMPTY;
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.iter().count(), 0);
    }

    #[test]
    fn insert_and_remove() {
        let mut s = PermissionSet::EMPTY;
        s.insert(Permission::Cron);
        assert!(s.contains(Permission::Cron));
        s.insert(Permission::File);
        assert_eq!(s.len(), 2);
        s.remove(Permission::Cron);
        assert!(!s.contains(Permission::Cron));
        assert_eq!(s.len(), 1);
    }
}
