//! Parser for `apt-get -s upgrade` simulate output. Each line of
//! the form `<name>/<old> <arch> <current> <candidate> ...` is
//! turned into a `PackageUpdate` with the kind derived from the
//! origin tag (security vs other).

use openpanel_domain::{PackageUpdate, UpdateKind};

/// Parse the simulated-upgrade output. Lines beginning with
/// `Inst` are the ones we want. A `-security` origin tag classifies
/// the update as `Security`; everything else is `Other`.
pub fn parse_apt_dry_run(output: &str) -> Vec<PackageUpdate> {
    let mut updates = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let rest = match line.strip_prefix("Inst ") {
            Some(rest) => rest,
            None => continue,
        };
        let mut parts = rest.split_whitespace();
        let name = match parts.next() {
            Some(name) => name,
            None => continue,
        };
        // Strip the trailing " (<reason>)" after the package name.
        let name = name.split('/').next().unwrap_or(name);
        let current = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let current = current
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(current);
        let candidate = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let candidate = candidate
            .strip_prefix('(')
            .and_then(|s| s.split_whitespace().next())
            .unwrap_or(candidate);
        // The remaining parts are the origin (e.g. "Debian:12.4/security")
        // and the architecture. We use the origin to classify security.
        let mut origin = String::new();
        for part in parts {
            if part.starts_with('[') {
                break;
            }
            if !origin.is_empty() {
                origin.push(' ');
            }
            origin.push_str(part);
        }
        let kind = if origin.contains("security") {
            UpdateKind::Security
        } else {
            UpdateKind::Other
        };
        let mut update = PackageUpdate::new(name, current, candidate, kind);
        update.summary = line.to_string();
        updates.push(update);
    }
    updates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_security_origin() {
        let output = "Inst libssl3 [3.0.11-1~deb12u1] (3.0.13-1~deb12u1 Debian:12.4/security [amd64])\n";
        let updates = parse_apt_dry_run(output);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "libssl3");
        assert_eq!(updates[0].current_version, "3.0.11-1~deb12u1");
        assert_eq!(updates[0].candidate_version, "3.0.13-1~deb12u1");
        assert_eq!(updates[0].kind, UpdateKind::Security);
    }

    #[test]
    fn parses_non_security_origin() {
        let output =
            "Inst bash [5.2.15-2] (5.2.15-2+b1 Debian:12.4 [amd64])\n";
        let updates = parse_apt_dry_run(output);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].kind, UpdateKind::Other);
    }

    #[test]
    fn skips_non_inst_lines() {
        let output = "Conf bash (5.2.15-2+b1 Debian:12.4 [amd64])\n";
        let updates = parse_apt_dry_run(output);
        assert!(updates.is_empty());
    }
}
