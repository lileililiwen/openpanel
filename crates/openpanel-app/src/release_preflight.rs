//! Release preflight: refuse to mutate a store whose schema is newer
//! than this binary knows how to apply.
//!
//! The `release-deployment-governance` spec requires the binary to
//! detect a too-new schema at startup and refuse to run rather than
//! silently corrupting the store. This module owns the pure decision
//! function; the async I/O that reads the `_migrations` ledger lives
//! in `openpanel_core::migration::MigrationRunner` and is consumed
//! by the composition root's startup sequence.

use std::collections::BTreeSet;

use openpanel_core::BinaryMetadata;
use thiserror::Error;

/// Decision returned by [`evaluate`]. Every variant carries the
/// information the operator needs to choose the right recovery path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightOutcome {
    /// Applied schema is within the binary's ceiling (or no ceiling
    /// was compiled in, the dev-build fallback). Startup may proceed.
    Ok,
    /// The store contains an applied schema version newer than this
    /// binary supports. Startup is refused; the operator MUST upgrade
    /// the binary (or roll the schema back via a backup).
    TooNewSchema {
        /// Newest applied version the ledger has seen.
        applied: String,
        /// Highest schema version this binary can apply.
        supported_max: String,
    },
    /// The store contains an applied version string that does not
    /// parse as a non-negative integer. The decision is conservative:
    /// refuse to mutate a store we cannot compare.
    UnparseableAppliedVersion {
        /// The version string that did not parse.
        version: String,
    },
}

impl PreflightOutcome {
    /// True when startup may proceed.
    pub fn is_ok(&self) -> bool {
        matches!(self, PreflightOutcome::Ok)
    }

    /// Short, operator-facing copy describing the recovery path. Safe
    /// to log; contains no secrets.
    pub fn operator_hint(&self) -> Option<&'static str> {
        match self {
            PreflightOutcome::Ok => None,
            PreflightOutcome::TooNewSchema { .. } => Some(
                "Refusing to start: the database schema is newer than this binary supports. \
                 Upgrade openpanel to a release whose OPENPANEL_MAX_SCHEMA_VERSION is at least \
                 as new as the applied schema, or restore the database from a backup taken \
                 with a compatible binary.",
            ),
            PreflightOutcome::UnparseableAppliedVersion { .. } => Some(
                "Refusing to start: the _migrations ledger contains a version string that \
                 does not parse as a non-negative integer. Inspect the ledger with a \
                 compatible openpanel build before retrying.",
            ),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
/// Errors raised by [`evaluate`] for malformed inputs the public
/// surface should never see (empty version string, etc.).
pub enum PreflightError {
    /// A version string in the ledger was empty.
    #[error("applied version is empty")]
    EmptyAppliedVersion,
}

/// The pure decision function. `applied` is the list of version
/// strings already in the `_migrations` ledger; `binary` is the
/// metadata block embedded in the running binary.
///
/// Rules (per the `release-deployment-governance` spec, "Safe Upgrade
/// and Rollback"):
///
/// 1. When `binary.max_schema_version_u32() == 0`, the binary has no
///    compiled ceiling (a dev build). Every applied version is
///    accepted; this is the pre-ratchet fallback.
/// 2. An applied version that does not parse as a non-negative
///    integer is treated as `UnparseableAppliedVersion` — refusing
///    to mutate a store we cannot compare is the conservative choice.
/// 3. Otherwise, if any applied version is strictly greater than
///    `binary.max_schema_version_u32()`, the outcome is
///    `TooNewSchema`. Two equal versions are accepted.
/// 4. An empty applied list is `Ok`.
pub fn evaluate(
    applied: &[String],
    binary: &BinaryMetadata,
) -> Result<PreflightOutcome, PreflightError> {
    for v in applied {
        if v.trim().is_empty() {
            return Err(PreflightError::EmptyAppliedVersion);
        }
    }

    let ceiling = binary.max_schema_version_u32();
    if ceiling == 0 {
        return Ok(PreflightOutcome::Ok);
    }

    let mut newest_seen: Option<u32> = None;
    for v in applied {
        match v.trim().parse::<u32>() {
            Ok(n) => {
                newest_seen = Some(newest_seen.map_or(n, |cur| cur.max(n)));
            }
            Err(_) => {
                return Ok(PreflightOutcome::UnparseableAppliedVersion { version: v.clone() });
            }
        }
    }

    match newest_seen {
        Some(n) if n > ceiling => Ok(PreflightOutcome::TooNewSchema {
            applied: n.to_string(),
            supported_max: ceiling.to_string(),
        }),
        _ => Ok(PreflightOutcome::Ok),
    }
}

/// Convenience wrapper that strips a `MigrationRunner` result down to
/// the slice this module needs. Composition-root code calls this
/// before any mutation runs.
pub async fn evaluate_runner(
    applied: Vec<String>,
    binary: &BinaryMetadata,
) -> Result<PreflightOutcome, PreflightError> {
    evaluate(&applied, binary)
}
/// Return the set of distinct applied versions, in ascending order.
/// Provided for callers that want a stable, printable summary without
/// re-sorting the input.
pub fn distinct_versions(applied: &[String]) -> BTreeSet<String> {
    applied.iter().cloned().collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use openpanel_core::BinaryMetadata;

    fn meta_with_ceiling(ceiling: &str) -> BinaryMetadata {
        let mut m = BinaryMetadata::load();
        m.max_schema_version = ceiling.to_string();
        m
    }

    #[test]
    fn empty_applied_is_ok() {
        let m = meta_with_ceiling("10");
        assert_eq!(evaluate(&[], &m), Ok(PreflightOutcome::Ok));
    }

    #[test]
    fn dev_build_with_zero_ceiling_accepts_any_known_version() {
        let m = meta_with_ceiling("0");
        let applied: Vec<String> = vec!["1".into(), "42".into(), "9999".into()];
        assert_eq!(evaluate(&applied, &m), Ok(PreflightOutcome::Ok));
    }

    #[test]
    fn equal_ceiling_is_ok() {
        let m = meta_with_ceiling("5");
        let applied: Vec<String> = vec!["1".into(), "5".into()];
        assert_eq!(evaluate(&applied, &m), Ok(PreflightOutcome::Ok));
    }

    #[test]
    fn one_version_above_ceiling_is_too_new() {
        let m = meta_with_ceiling("5");
        let applied: Vec<String> = vec!["1".into(), "6".into()];
        assert_eq!(
            evaluate(&applied, &m),
            Ok(PreflightOutcome::TooNewSchema {
                applied: "6".into(),
                supported_max: "5".into(),
            })
        );
    }

    #[test]
    fn far_ahead_ceiling_is_too_new() {
        let m = meta_with_ceiling("5");
        let applied: Vec<String> = vec!["100".into()];
        assert_eq!(
            evaluate(&applied, &m),
            Ok(PreflightOutcome::TooNewSchema {
                applied: "100".into(),
                supported_max: "5".into(),
            })
        );
    }

    #[test]
    fn unparseable_version_is_reported() {
        let m = meta_with_ceiling("5");
        let applied: Vec<String> = vec!["not-a-number".into()];
        assert_eq!(
            evaluate(&applied, &m),
            Ok(PreflightOutcome::UnparseableAppliedVersion {
                version: "not-a-number".into(),
            })
        );
    }

    #[test]
    fn empty_version_string_is_a_preflight_error() {
        let m = meta_with_ceiling("5");
        let applied: Vec<String> = vec!["".into()];
        assert_eq!(
            evaluate(&applied, &m),
            Err(PreflightError::EmptyAppliedVersion)
        );
    }

    #[test]
    fn recovery_copy_is_present_for_failures() {
        let m = meta_with_ceiling("5");
        let too_new = evaluate(["6".into()].as_slice(), &m).unwrap();
        assert!(too_new.operator_hint().is_some());
        assert!(!too_new.is_ok());

        let unparseable = evaluate(["abc".into()].as_slice(), &meta_with_ceiling("5")).unwrap();
        assert!(unparseable.operator_hint().is_some());
        assert!(!unparseable.is_ok());

        let ok = evaluate(["5".into()].as_slice(), &m).unwrap();
        assert!(ok.operator_hint().is_none());
        assert!(ok.is_ok());
    }

    #[test]
    fn distinct_versions_is_sorted_and_deduped() {
        let applied: Vec<String> = vec!["3".into(), "1".into(), "3".into(), "2".into(), "1".into()];
        let out = distinct_versions(&applied);
        let got: Vec<String> = out.into_iter().collect();
        assert_eq!(got, vec!["1", "2", "3"]);
    }

    #[test]
    fn unparseable_version_beats_too_new_when_mixed() {
        let m = meta_with_ceiling("5");
        let applied: Vec<String> = vec!["3".into(), "abc".into(), "7".into()];
        assert_eq!(
            evaluate(&applied, &m),
            Ok(PreflightOutcome::UnparseableAppliedVersion {
                version: "abc".into(),
            })
        );
    }

    #[test]
    fn whitespace_around_numeric_version_is_tolerated() {
        let m = meta_with_ceiling("5");
        let applied: Vec<String> = vec!["  4  ".into()];
        assert_eq!(evaluate(&applied, &m), Ok(PreflightOutcome::Ok));
    }
}
