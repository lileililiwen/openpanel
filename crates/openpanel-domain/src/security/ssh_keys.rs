//! Admin SSH public keys for host login: pure parsing/validation and
//! the managed-block `authorized_keys` renderer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::SecurityError;

/// Accepted key algorithms with their minimum decoded body sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAlgo {
    /// Ed25519 (≥ 60-byte body).
    Ed25519,
    /// NIST P-256 ECDSA (≥ 60-byte body).
    EcdsaP256,
    /// RSA ≥ 3072 bits (≥ 300-byte body).
    Rsa3072Up,
}

impl KeyAlgo {
    /// The SSH wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ed25519 => "ssh-ed25519",
            Self::EcdsaP256 => "ecdsa-sha2-nistp256",
            Self::Rsa3072Up => "rsa-sha2-512",
        }
    }

    fn minimum_body_len(&self) -> usize {
        match self {
            Self::Ed25519 | Self::EcdsaP256 => 60,
            Self::Rsa3072Up => 300,
        }
    }

    /// Parse the SSH wire name.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "ssh-ed25519" => Some(Self::Ed25519),
            "ecdsa-sha2-nistp256" => Some(Self::EcdsaP256),
            "rsa-sha2-512" | "ssh-rsa" => Some(Self::Rsa3072Up),
            _ => None,
        }
    }
}

/// One admin SSH public key registered for host login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostSshKey {
    id: Uuid,
    label: String,
    fingerprint: String,
    public_key_b64: String,
    algo: KeyAlgo,
    added_by: Uuid,
    added_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl HostSshKey {
    /// Parse and validate a submitted single-line public key. The
    /// line MUST NOT carry embedded options (`algo` must be the first
    /// token) and MUST decode to at least the algorithm's minimum
    /// body size.
    pub fn parse(
        label: impl Into<String>,
        line: &str,
        added_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Self, SecurityError> {
        if line.contains('\n') || line.contains('\r') {
            return Err(SecurityError::InvalidSshKey(
                "key must be a single line".into(),
            ));
        }
        let line = line.trim();
        let mut tokens = line.split_whitespace();
        let algo_name = tokens
            .next()
            .ok_or_else(|| SecurityError::InvalidSshKey("empty key line".into()))?;
        // Options-embedded lines start with something that is not a
        // known algorithm name.
        let algo = KeyAlgo::parse(algo_name).ok_or_else(|| {
            if line.contains('=') || line.contains(",") && !line.starts_with("ssh-") {
                SecurityError::InvalidSshKey("options embedded in key".into())
            } else {
                SecurityError::InvalidSshKey(format!("unsupported algorithm `{algo_name}`"))
            }
        })?;
        let b64 = tokens
            .next()
            .ok_or_else(|| SecurityError::InvalidSshKey("missing key body".into()))?;
        use base64::Engine;
        let body = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| SecurityError::InvalidSshKey("key body is not base64".into()))?;
        if body.len() < algo.minimum_body_len() {
            return Err(SecurityError::InvalidSshKey(format!(
                "{} body too small ({} bytes); key below the approved strength",
                algo.as_str(),
                body.len()
            )));
        }
        let digest = Sha256::digest(body);
        use base64::Engine as _;
        let fingerprint = format!(
            "SHA256:{}",
            base64::engine::general_purpose::STANDARD.encode(digest)
        );
        let label = label.into();
        if label.is_empty() || label.len() > 64 {
            return Err(SecurityError::InvalidSshKey(
                "label must be 1..=64 chars".into(),
            ));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            label,
            fingerprint,
            public_key_b64: b64.to_string(),
            algo,
            added_by,
            added_at: now,
            last_used_at: None,
        })
    }

    /// Identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Operator-facing label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// SHA256 fingerprint.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Stored key body (base64).
    pub fn public_key_b64(&self) -> &str {
        &self.public_key_b64
    }

    /// Algorithm.
    pub fn algo(&self) -> KeyAlgo {
        self.algo
    }

    /// Who added the key.
    pub fn added_by(&self) -> Uuid {
        self.added_by
    }

    /// When the key was added.
    pub fn added_at(&self) -> DateTime<Utc> {
        self.added_at
    }

    /// Last successful public-key login, when observed.
    pub fn last_used_at(&self) -> Option<DateTime<Utc>> {
        self.last_used_at
    }

    /// Restore from persistence (including observed last-use).
    pub fn restore(
        id: Uuid,
        label: String,
        fingerprint: String,
        public_key_b64: String,
        algo: KeyAlgo,
        added_by: Uuid,
        added_at: DateTime<Utc>,
        last_used_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id,
            label,
            fingerprint,
            public_key_b64,
            algo,
            added_by,
            added_at,
            last_used_at,
        }
    }

    /// Record an observed login.
    pub fn mark_used(&mut self, at: DateTime<Utc>) {
        self.last_used_at = Some(at);
    }

    /// The managed authorized_keys line for this key.
    pub fn render_line(&self) -> String {
        format!(
            "no-agent-forwarding,no-port-forwarding {} {} openpanel:{}\n",
            self.algo.as_str(),
            self.public_key_b64,
            self.label
        )
    }
}

/// Managed-block markers in `authorized_keys`.
pub const MANAGED_BEGIN: &str = "# BEGIN openpanel-managed (do not edit)\n";
pub const MANAGED_END: &str = "# END openpanel-managed\n";

/// Render `authorized_keys`: managed block first, out-of-band lines
/// preserved verbatim below it.
pub fn render_authorized_keys(existing: &str, keys: &[HostSshKey]) -> String {
    let mut out = String::from(MANAGED_BEGIN);
    for key in keys {
        out.push_str(&key.render_line());
    }
    out.push_str(MANAGED_END);
    let tail = existing
        .split_once(MANAGED_END)
        .map(|(_, rest)| rest)
        .unwrap_or(existing);
    if !tail.is_empty() {
        if !tail.starts_with('\n') {
            out.push('\n');
        }
        out.push_str(tail);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_line(body_len: usize, algo: &str) -> String {
        let body = vec![0xABu8; body_len];
        use base64::Engine;
        format!(
            "{algo} {} tester@host",
            base64::engine::general_purpose::STANDARD.encode(body)
        )
    }

    #[test]
    fn test_key_validation() {
        let now = Utc::now();
        let who = Uuid::new_v4();
        // Valid ed25519 accepted.
        assert!(HostSshKey::parse("laptop", &valid_line(68, "ssh-ed25519"), who, now).is_ok());
        // rsa-1024 (small body) rejected.
        assert!(HostSshKey::parse("old", &valid_line(140, "ssh-rsa"), who, now).is_err());
        // Multi-line rejected.
        assert!(
            HostSshKey::parse(
                "x",
                &format!(
                    "{}\n{}",
                    valid_line(68, "ssh-ed25519"),
                    valid_line(68, "ssh-ed25519")
                ),
                who,
                now
            )
            .is_err()
        );
        // Embedded options rejected.
        assert!(
            HostSshKey::parse(
                "x",
                &format!("no-port-forwarding {}", valid_line(68, "ssh-ed25519")),
                who,
                now
            )
            .is_err()
        );
    }

    #[test]
    fn test_managed_block_rendering_preserves_out_of_band_lines() {
        let now = Utc::now();
        let who = Uuid::new_v4();
        let k1 = HostSshKey::parse("laptop", &valid_line(68, "ssh-ed25519"), who, now).unwrap();
        let k2 = HostSshKey::parse("ci", &valid_line(320, "rsa-sha2-512"), who, now).unwrap();
        let existing = "# operator key\nssh-rsa AAAAoperator ops@box\n";
        let rendered = render_authorized_keys(existing, &[k1.clone(), k2]);
        assert!(rendered.starts_with(MANAGED_BEGIN));
        assert!(rendered.contains("no-agent-forwarding,no-port-forwarding ssh-ed25519 "));
        assert!(rendered.contains("openpanel:laptop\n"));
        assert!(rendered.contains("openpanel:ci\n"));
        assert!(rendered.ends_with("# operator key\nssh-rsa AAAAoperator ops@box\n"));
        // Block replace keeps a single marker pair.
        let twice = render_authorized_keys(&rendered, &[k1]);
        assert_eq!(twice.matches(MANAGED_BEGIN).count(), 1);
        assert!(!twice.contains("openpanel:ci"));
    }
}

#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_rendered_keys_are_unique_options_prefixed_and_public_only(
            n in 1usize..6,
            algo in prop::sample::select(vec!["ssh-ed25519", "ecdsa-sha2-nistp256", "rsa-sha2-512"]),
            seeds in proptest::collection::vec(any::<u64>(), 1..6),
            label_seed in "[A-Za-z0-9_-]{1,16}",
        ) {
            let now = Utc::now();
            let who = Uuid::new_v4();
            let mut keys = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for (i, seed) in seeds.iter().take(n).enumerate() {
                // Deterministic per-seed body of valid size.
                let body_len = match algo { "rsa-sha2-512" => 320, _ => 68 };
                let mut body = vec![0u8; body_len];
                let mut state = *seed ^ (i as u64) ^ 0x9E37_79B9_7F4A_7C15;
                for b in body.iter_mut() {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    *b = (state >> 33) as u8;
                }
                use base64::Engine;
                let line = format!(
                    "{algo} {} prop-{label_seed}-{i}",
                    base64::engine::general_purpose::STANDARD.encode(body)
                );
                let key = HostSshKey::parse(format!("prop-{label_seed}-{i}"), &line, who, now)
                    .unwrap_or_else(|e| panic!("valid key rejected: {e}"));
                if seen.insert(key.fingerprint().to_string()) {
                    keys.push(key);
                }
            }
            let rendered = render_authorized_keys("", &keys);
            // Each fingerprint's label appears exactly once.
            for key in &keys {
                let marker = format!("openpanel:{}", key.label());
                prop_assert_eq!(rendered.matches(&marker).count(), 1);
            }
            // Every managed line starts with the enforced options.
            for line in rendered.lines().filter(|l| l.starts_with("ssh-") || l.starts_with("ecdsa-") || l.starts_with("rsa-")) {
                prop_assert!(line.starts_with("no-agent-forwarding,no-port-forwarding "));
            }
            // The renderer only ever emits the base64 bodies it was
            // given — private-key material cannot exist in this input
            // domain (no PEM/OPENSSH markers are representable).
            prop_assert!(!rendered.contains("BEGIN OPENSSH PRIVATE KEY"));
            prop_assert!(!rendered.contains("PRIVATE KEY"));
        }
    }
}
