//! Registry host adapter port: the boundary the container
//! runtime service uses to talk to a remote OCI Distribution
//! registry. Production wires a `bollard`-backed impl; tests
//! wire `NoopRegistryHostAdapter`.

use async_trait::async_trait;
use uuid::Uuid;

/// Pull request for one image from a remote registry.
#[derive(Debug, Clone)]
pub struct PullRequest {
    /// Caller's user id (audit context).
    pub caller: Uuid,
    /// Container id this pull is for (audit context).
    pub container_id: Uuid,
    /// Image reference (`repo[:tag]` or `repo@sha256:...`).
    pub image_ref: String,
    /// Optional registry credential id; when `None`, anonymous
    /// pull is attempted.
    pub registry_credential_id: Option<Uuid>,
    /// Plaintext registry username to use for the pull (resolved
    /// by the service from the stored credential).
    pub username: Option<String>,
    /// Plaintext registry password / PAT (resolved by the
    /// service from the stored credential).
    pub password: Option<String>,
}

/// Result of a successful pull: a stable reference-count marker
/// and the resolved image digest.
#[derive(Debug, Clone)]
pub struct PullResult {
    /// Image digest returned by the registry.
    pub image_digest: String,
    /// Reference count of the image on the registry (mirrors
    /// the spec scenario's `ref_count` audit field).
    pub ref_count: u32,
}

/// Errors raised by a registry pull attempt.
#[derive(Debug, thiserror::Error)]
pub enum PullError {
    /// Registry authentication failed (401). The adapter MUST
    /// redact the upstream reason before returning it to the
    /// service so the audit never echoes bare upstream bodies.
    #[error("registry auth failed (redacted reason)")]
    AuthFailed,
    /// The adapter is unavailable (network, daemon down).
    #[error("adapter unavailable: {0}")]
    Unavailable(String),
    /// The image ref is malformed.
    #[error("bad image ref: {0}")]
    BadImageRef(String),
    /// Generic upstream failure already redacted by the
    /// adapter.
    #[error("registry failed: {0}")]
    Other(String),
}

impl From<PullError> for openpanel_domain::ContainerRuntimeError {
    fn from(e: PullError) -> Self {
        match e {
            PullError::AuthFailed => openpanel_domain::ContainerRuntimeError::AdapterUnavailable(
                "registry auth failed".into(),
            ),
            PullError::Unavailable(m) => {
                openpanel_domain::ContainerRuntimeError::AdapterUnavailable(m)
            }
            PullError::BadImageRef(m) => openpanel_domain::ContainerRuntimeError::InvalidCipher(m),
            PullError::Other(m) => openpanel_domain::ContainerRuntimeError::AdapterUnavailable(m),
        }
    }
}

/// Port implemented by the registry host adapter.
#[async_trait]
pub trait RegistryHostAdapter: Send + Sync + 'static {
    /// Pull a single image with the supplied credentials. The
    /// adapter is responsible for redacting any upstream error
    /// bodies before exposing them.
    async fn pull(&self, request: PullRequest) -> Result<PullResult, PullError>;
}

/// No-op adapter used by tests and the offline CLI/agent builds.
/// Always succeeds with a deterministic digest derived from the
/// `image_ref`.
pub struct NoopRegistryHostAdapter;

#[async_trait]
impl RegistryHostAdapter for NoopRegistryHostAdapter {
    async fn pull(&self, request: PullRequest) -> Result<PullResult, PullError> {
        // Derive a stable pseudo-digest so the service can audit
        // a deterministic ref-count across tests.
        let digest = format!("sha256:{}", pseudo_hash(&request.image_ref));
        Ok(PullResult {
            image_digest: digest,
            ref_count: 1,
        })
    }
}

fn pseudo_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8222_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_pull_returns_deterministic_digest() {
        let adapter = NoopRegistryHostAdapter;
        let req = PullRequest {
            caller: Uuid::new_v4(),
            container_id: Uuid::new_v4(),
            image_ref: "registry.example.com/foo:latest".into(),
            registry_credential_id: None,
            username: None,
            password: None,
        };
        let r1 = adapter.pull(req.clone()).await.unwrap();
        let r2 = adapter.pull(req).await.unwrap();
        assert_eq!(r1.image_digest, r2.image_digest);
    }
}
