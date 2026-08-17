//! Vulnerability-scan result.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::image::ImageDigest;

/// A single scan finding. Severity is informational; the registry
/// never auto-deletes on findings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanFinding {
    /// CVE / advisory identifier.
    pub id: String,
    /// Severity (`low` / `medium` / `high` / `critical`).
    pub severity: String,
    /// Human-readable summary.
    pub summary: String,
}

/// Aggregated scan result for an image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanResult {
    /// Image the scan covers.
    pub digest: ImageDigest,
    /// When the scan completed.
    pub scanned_at: DateTime<Utc>,
    /// Individual findings.
    pub findings: Vec<ScanFinding>,
}

impl ScanResult {
    /// Construct a new scan result.
    pub fn new(digest: ImageDigest, scanned_at: DateTime<Utc>, findings: Vec<ScanFinding>) -> Self {
        Self {
            digest,
            scanned_at,
            findings,
        }
    }

    /// Returns `true` if the scan reported any findings.
    pub fn has_findings(&self) -> bool {
        !self.findings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_result_has_findings() {
        let digest = ImageDigest::new(format!("sha256:{}", "b".repeat(64))).unwrap();
        let r = ScanResult::new(
            digest,
            Utc::now(),
            vec![ScanFinding {
                id: "CVE-2099-99999".into(),
                severity: "high".into(),
                summary: "demo".into(),
            }],
        );
        assert!(r.has_findings());
    }
}
