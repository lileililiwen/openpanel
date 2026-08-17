//! Retention policy.
//!
//! A retention policy can constrain either the maximum number of
//! images kept per namespace OR the maximum age of any image. A
//! namespace is "in violation" if it exceeds either bound.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Retention policy: prune over-count or over-age images after each
/// push.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPolicy {
    /// Maximum images per namespace. `None` disables count-based
    /// pruning.
    pub max_images_per_ns: Option<u32>,
    /// Maximum age in days. `None` disables age-based pruning.
    pub max_age_days: Option<u32>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_images_per_ns: Some(50),
            max_age_days: None,
        }
    }
}

/// Verdict from a retention check: the list of images that should be
/// pruned to bring the namespace back into compliance.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RetentionVerdict {
    /// Digests that should be deleted.
    pub to_delete: Vec<String>,
}

impl RetentionPolicy {
    /// Construct a count-only retention policy.
    pub fn by_count(max: u32) -> Self {
        Self {
            max_images_per_ns: Some(max),
            max_age_days: None,
        }
    }

    /// Construct an age-only retention policy.
    pub fn by_age(max_age_days: u32) -> Self {
        Self {
            max_images_per_ns: None,
            max_age_days: Some(max_age_days),
        }
    }

    /// Apply the policy to a list of `(digest, pushed_at)` pairs in
    /// arbitrary order. The verdict lists digests to delete, oldest
    /// first for age pruning and over-count for count pruning.
    pub fn apply<I>(&self, images: I, now: DateTime<Utc>) -> RetentionVerdict
    where
        I: IntoIterator<Item = (String, DateTime<Utc>)>,
    {
        let mut entries: Vec<(String, DateTime<Utc>)> = images.into_iter().collect();
        // Sort descending by pushed_at so the newest is first; the
        // tail is the oldest (and the first to be pruned for count
        // limits).
        entries.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
        let mut to_delete = Vec::new();
        if let Some(max_age) = self.max_age_days {
            let max_age = chrono::Duration::days(max_age as i64);
            for (digest, ts) in &entries {
                if now - *ts > max_age {
                    to_delete.push(digest.clone());
                }
            }
        }
        if let Some(max_count) = self.max_images_per_ns {
            let over = entries.len().saturating_sub(max_count as usize);
            // Walk from the tail (oldest) upward.
            for (digest, _) in entries.iter().rev().take(over) {
                if !to_delete.contains(digest) {
                    to_delete.push(digest.clone());
                }
            }
        }
        RetentionVerdict { to_delete }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn entry(label: &str, age_days: i64) -> (String, DateTime<Utc>) {
        let now = Utc::now();
        (label.to_string(), now - Duration::days(age_days))
    }

    #[test]
    fn count_policy_prunes_oldest_first() {
        let policy = RetentionPolicy::by_count(2);
        let now = Utc::now();
        let entries = vec![entry("a", 0), entry("b", 1), entry("c", 2)];
        let verdict = policy.apply(entries, now);
        assert_eq!(verdict.to_delete.len(), 1);
        // `a` is 0 days old (newest), `c` is 2 days old (oldest).
        // The oldest must be the first pruned.
        assert_eq!(verdict.to_delete[0], "c");
    }

    #[test]
    fn age_policy_prunes_over_age() {
        let policy = RetentionPolicy::by_age(7);
        let now = Utc::now();
        let entries = vec![entry("a", 0), entry("b", 10), entry("c", 30)];
        let verdict = policy.apply(entries, now);
        assert_eq!(verdict.to_delete.len(), 2);
        assert!(verdict.to_delete.contains(&"b".to_string()));
        assert!(verdict.to_delete.contains(&"c".to_string()));
    }

    #[test]
    fn empty_policy_never_prunes() {
        let policy = RetentionPolicy {
            max_images_per_ns: None,
            max_age_days: None,
        };
        let now = Utc::now();
        let entries = vec![entry("a", 0), entry("b", 30)];
        let verdict = policy.apply(entries, now);
        assert!(verdict.to_delete.is_empty());
    }

    #[test]
    fn combined_policy_prunes_age_and_overflow() {
        let policy = RetentionPolicy {
            max_images_per_ns: Some(3),
            max_age_days: Some(7),
        };
        let now = Utc::now();
        let entries = vec![
            entry("a", 0),
            entry("b", 1),
            entry("c", 2),
            entry("d", 10),
            entry("e", 20),
        ];
        let verdict = policy.apply(entries, now);
        assert!(verdict.to_delete.contains(&"d".to_string()));
        assert!(verdict.to_delete.contains(&"e".to_string()));
    }
}
