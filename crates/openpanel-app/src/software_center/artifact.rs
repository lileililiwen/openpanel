//! Checksummed, resource-bounded extraction for pinned official application archives.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use futures_util::StreamExt;
use openpanel_domain::software_center::{ArchivePolicy, ArtifactPin};
use sha1::Sha1;
use sha2::{Digest, Sha256};

use super::SoftwareCenterError;

/// Pinned archive digest from an official release recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDigest {
    algorithm: DigestAlgorithm,
    expected: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DigestAlgorithm {
    Sha1,
    Sha256,
}

/// Placeholder SHA-256 documented in the recovery seed for entries whose
/// upstream does not publish a hash. The first 64 hex digits of this
/// value are all zero; downloaders treat it as "no digest supplied" and
/// skip the verification step instead of failing the install.
pub const PLACEHOLDER_SHA256: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

impl ArtifactDigest {
    /// Construct a SHA-256 digest from exactly 64 hexadecimal characters.
    pub fn sha256(expected: &str) -> Result<Self, SoftwareCenterError> {
        Self::new(DigestAlgorithm::Sha256, expected, 64)
    }

    /// Construct an upstream-published SHA-1 digest from exactly 40 hexadecimal characters.
    pub fn sha1(expected: &str) -> Result<Self, SoftwareCenterError> {
        Self::new(DigestAlgorithm::Sha1, expected, 40)
    }

    fn new(
        algorithm: DigestAlgorithm,
        expected: &str,
        length: usize,
    ) -> Result<Self, SoftwareCenterError> {
        if expected.len() != length || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(SoftwareCenterError::Invalid);
        }
        Ok(Self {
            algorithm,
            expected: expected.to_ascii_lowercase(),
        })
    }

    fn verify(&self, bytes: &[u8]) -> bool {
        let actual = match self.algorithm {
            DigestAlgorithm::Sha1 => hex::encode(Sha1::digest(bytes)),
            DigestAlgorithm::Sha256 => hex::encode(Sha256::digest(bytes)),
        };
        actual == self.expected
    }
}

/// Safe tar.gz extraction policy used only inside a new empty staging directory.
pub struct SafeArtifactInstaller {
    maximum_archive_bytes: usize,
    archive_policy: ArchivePolicy,
}

/// One immutable official release artifact in the recovery catalog.
#[derive(Debug, Clone)]
pub struct PinnedArtifact {
    /// Exact HTTPS download URL.
    pub url: &'static str,
    /// Published digest.
    pub digest: ArtifactDigest,
    /// Single archive directory stripped during staged extraction.
    pub archive_root: &'static str,
}

/// Outcome of placing one artifact on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedArtifact {
    /// Absolute path to the placed directory (for archives) or file
    /// (for `archive_type == "file"`).
    pub path: PathBuf,
    /// Filename of the placed file when the artifact is a single file.
    pub filename: Option<String>,
    /// Whether the digest check was skipped because the recipe only
    /// ships a documented placeholder.
    pub digest_verified: bool,
}

impl PlacedArtifact {
    fn file(path: PathBuf, filename: String, digest_verified: bool) -> Self {
        Self {
            path,
            filename: Some(filename),
            digest_verified,
        }
    }

    fn directory(path: PathBuf, digest_verified: bool) -> Self {
        Self {
            path,
            filename: None,
            digest_verified,
        }
    }
}

/// Async byte fetcher for one artifact URL. Production wires the
/// `ReqwestArtifactFetcher`; tests inject a `MemoryArtifactFetcher` that
/// returns a pre-baked byte stream.
#[async_trait::async_trait]
pub trait ArtifactFetcher: Send + Sync {
    /// Fetch the bytes for `url` with a hard size cap. Redirects are
    /// refused; only `https://` (or `http://` for the in-process test
    /// server) is accepted.
    async fn fetch(&self, url: &str) -> Result<Vec<u8>, SoftwareCenterError>;
}

/// Production fetcher backed by a TLS-only reqwest client with no
/// redirect-following. Used for every real install path.
pub struct ReqwestArtifactFetcher {
    client: reqwest::Client,
    maximum_archive_bytes: usize,
}

impl ReqwestArtifactFetcher {
    /// Construct a TLS-only client that refuses redirects.
    pub fn new() -> Result<Self, SoftwareCenterError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_| SoftwareCenterError::Package)?;
        Ok(Self {
            client,
            maximum_archive_bytes: 64 * 1024 * 1024,
        })
    }
}

impl Default for ReqwestArtifactFetcher {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            client: reqwest::Client::new(),
            maximum_archive_bytes: 64 * 1024 * 1024,
        })
    }
}

#[async_trait::async_trait]
impl ArtifactFetcher for ReqwestArtifactFetcher {
    async fn fetch(&self, url: &str) -> Result<Vec<u8>, SoftwareCenterError> {
        validate_artifact_url(url)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| SoftwareCenterError::Package)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > self.maximum_archive_bytes as u64)
        {
            return Err(SoftwareCenterError::Package);
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| SoftwareCenterError::Package)?;
            if bytes.len().saturating_add(chunk.len()) > self.maximum_archive_bytes {
                return Err(SoftwareCenterError::Invalid);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

/// Place a single artifact on disk regardless of its kind. The function
/// is the only place that knows about archive vs. single-file layout.
/// `destination_root` is the managed base directory (`/var/lib/openpanel/webapps`).
pub fn place_artifact(
    pin: &ArtifactPin,
    bytes: &[u8],
    destination_root: &Path,
) -> Result<PlacedArtifact, SoftwareCenterError> {
    if bytes.is_empty() {
        return Err(SoftwareCenterError::Invalid);
    }
    let digest_verified = pin.sha256 != PLACEHOLDER_SHA256 && verify_sha256(pin, bytes);
    if pin.sha256 != PLACEHOLDER_SHA256 && !digest_verified {
        return Err(SoftwareCenterError::Invalid);
    }
    let version_dir = destination_root.join(safe_segment(pin.url.as_str())?);
    fs::create_dir_all(&version_dir).map_err(|_| SoftwareCenterError::Package)?;
    match pin.archive_type.as_str() {
        "file" => place_single_file(pin, bytes, &version_dir, digest_verified),
        "tar.gz" => place_tar_gz(pin, bytes, &version_dir, digest_verified),
        other => Err(unsupported_archive(other)),
    }
}

fn place_single_file(
    pin: &ArtifactPin,
    bytes: &[u8],
    destination: &Path,
    digest_verified: bool,
) -> Result<PlacedArtifact, SoftwareCenterError> {
    let filename = if !pin.archive_root.is_empty() {
        pin.archive_root.clone()
    } else {
        derive_filename_from_url(pin.url.as_str()).ok_or(SoftwareCenterError::Invalid)?
    };
    if filename.contains('/') || filename.contains('\\') || filename.is_empty() {
        return Err(SoftwareCenterError::Invalid);
    }
    let target = destination.join(&filename);
    if target.exists() {
        return Err(SoftwareCenterError::Conflict);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|_| SoftwareCenterError::Package)?;
    file.write_all(bytes)
        .map_err(|_| SoftwareCenterError::Package)?;
    file.flush().map_err(|_| SoftwareCenterError::Package)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644))
            .map_err(|_| SoftwareCenterError::Package)?;
    }
    Ok(PlacedArtifact::file(target, filename, digest_verified))
}

fn place_tar_gz(
    pin: &ArtifactPin,
    bytes: &[u8],
    destination: &Path,
    digest_verified: bool,
) -> Result<PlacedArtifact, SoftwareCenterError> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let expected_root = pin.archive_root.as_str();
    if expected_root.is_empty() || expected_root.contains('/') {
        return Err(SoftwareCenterError::Invalid);
    }
    let mut expanded_bytes = 0_u64;
    let mut file_count = 0_u64;
    let policy =
        ArchivePolicy::new(50_000, 512 * 1024 * 1024).map_err(|_| SoftwareCenterError::Invalid)?;
    for item in archive
        .entries()
        .map_err(|_| SoftwareCenterError::Invalid)?
    {
        let mut entry = item.map_err(|_| SoftwareCenterError::Invalid)?;
        let path = entry.path().map_err(|_| SoftwareCenterError::Invalid)?;
        let mut components = path.components();
        if components.next().and_then(|part| part.as_os_str().to_str()) != Some(expected_root) {
            return Err(SoftwareCenterError::Invalid);
        }
        let relative = components.as_path();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(SoftwareCenterError::Invalid);
        }
        file_count = file_count
            .checked_add(1)
            .ok_or(SoftwareCenterError::Invalid)?;
        expanded_bytes = expanded_bytes
            .checked_add(entry.size())
            .ok_or(SoftwareCenterError::Invalid)?;
        let relative_text = relative.to_str().ok_or(SoftwareCenterError::Invalid)?;
        policy
            .accept(relative_text, false, expanded_bytes, file_count)
            .map_err(|_| SoftwareCenterError::Invalid)?;
        let output = destination.join(relative);
        if kind.is_dir() {
            fs::create_dir_all(&output).map_err(|_| SoftwareCenterError::Package)?;
            continue;
        }
        let parent = output.parent().ok_or(SoftwareCenterError::Invalid)?;
        fs::create_dir_all(parent).map_err(|_| SoftwareCenterError::Package)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|_| SoftwareCenterError::Package)?;
        let written =
            std::io::copy(&mut entry, &mut file).map_err(|_| SoftwareCenterError::Package)?;
        if written != entry.size() {
            return Err(SoftwareCenterError::Invalid);
        }
        file.flush().map_err(|_| SoftwareCenterError::Package)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output, fs::Permissions::from_mode(0o644))
                .map_err(|_| SoftwareCenterError::Package)?;
        }
    }
    Ok(PlacedArtifact::directory(
        destination.to_path_buf(),
        digest_verified,
    ))
}

fn verify_sha256(pin: &ArtifactPin, bytes: &[u8]) -> bool {
    let digest = match ArtifactDigest::sha256(&pin.sha256) {
        Ok(value) => value,
        Err(_) => return false,
    };
    digest.verify(bytes)
}

fn derive_filename_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    let basename = path.rsplit('/').next().unwrap_or("");
    if basename.is_empty() {
        None
    } else {
        Some(basename.to_owned())
    }
}

fn safe_segment(url: &str) -> Result<String, SoftwareCenterError> {
    let mut segment = String::new();
    for byte in url.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                segment.push(byte as char);
            }
            _ => segment.push('_'),
        }
    }
    if segment.is_empty() || segment.len() > 128 {
        return Err(SoftwareCenterError::Invalid);
    }
    Ok(segment)
}

fn unsupported_archive(kind: &str) -> SoftwareCenterError {
    let _ = kind;
    SoftwareCenterError::Unsupported
}

fn validate_artifact_url(url: &str) -> Result<(), SoftwareCenterError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| SoftwareCenterError::Invalid)?;
    if !matches!(parsed.scheme(), "https" | "http")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(SoftwareCenterError::Invalid);
    }
    Ok(())
}

/// Return the pinned official archive recipe for a supported application.
pub fn pinned_artifact(application: &str) -> Result<PinnedArtifact, SoftwareCenterError> {
    match application {
        "wordpress" => Ok(PinnedArtifact {
            url: "https://wordpress.org/wordpress-7.0.3.tar.gz",
            digest: ArtifactDigest::sha1("344b74d7cbf13c55ba0f12cad207c06cfee4368a")?,
            archive_root: "wordpress",
        }),
        "drupal" => Ok(PinnedArtifact {
            url: "https://ftp.drupal.org/files/projects/drupal-11.3.12.tar.gz",
            digest: ArtifactDigest::sha256(
                "2d9653818d096be3c829fd70e986b9e65a61c67642a9a05e15f4f1affba5ac97",
            )?,
            archive_root: "drupal-11.3.12",
        }),
        _ => Err(SoftwareCenterError::Invalid),
    }
}

/// Redirect-free streaming HTTP downloader restricted to official catalog origins.
pub struct ArtifactDownloader {
    client: reqwest::Client,
    maximum_archive_bytes: usize,
}
impl ArtifactDownloader {
    /// Construct a TLS-only client that refuses redirects.
    pub fn new() -> Result<Self, SoftwareCenterError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_| SoftwareCenterError::Package)?;
        Ok(Self {
            client,
            maximum_archive_bytes: 64 * 1024 * 1024,
        })
    }

    /// Stream and bound one pinned artifact, then verify its digest before returning bytes.
    pub async fn download(
        &self,
        artifact: &PinnedArtifact,
    ) -> Result<Vec<u8>, SoftwareCenterError> {
        validate_artifact_url(artifact.url)?;
        let response = self
            .client
            .get(artifact.url)
            .send()
            .await
            .map_err(|_| SoftwareCenterError::Package)?;
        if !response.status().is_success() {
            return Err(SoftwareCenterError::Package);
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| SoftwareCenterError::Package)?;
            if bytes.len().saturating_add(chunk.len()) > self.maximum_archive_bytes {
                return Err(SoftwareCenterError::Invalid);
            }
            bytes.extend_from_slice(&chunk);
        }
        if !artifact.digest.verify(&bytes) {
            return Err(SoftwareCenterError::Invalid);
        }
        Ok(bytes)
    }
}

impl Default for SafeArtifactInstaller {
    fn default() -> Self {
        let archive_policy =
            ArchivePolicy::new(50_000, 512 * 1024 * 1024).unwrap_or_else(|_| std::process::abort());
        Self {
            maximum_archive_bytes: 64 * 1024 * 1024,
            archive_policy,
        }
    }
}
impl SafeArtifactInstaller {
    /// Verify an in-memory bounded download and extract its one expected root directory.
    pub fn extract_verified(
        &self,
        bytes: &[u8],
        digest: &ArtifactDigest,
        expected_root: &str,
        destination: &Path,
    ) -> Result<(), SoftwareCenterError> {
        if bytes.is_empty()
            || bytes.len() > self.maximum_archive_bytes
            || !digest.verify(bytes)
            || expected_root.is_empty()
            || expected_root.contains('/')
            || fs::symlink_metadata(destination)
                .map(|meta| !meta.is_dir() || meta.file_type().is_symlink())
                .unwrap_or(true)
            || fs::read_dir(destination)
                .map_err(|_| SoftwareCenterError::Package)?
                .next()
                .is_some()
        {
            return Err(SoftwareCenterError::Invalid);
        }
        let decoder = GzDecoder::new(bytes);
        let mut archive = tar::Archive::new(decoder);
        let mut expanded_bytes = 0_u64;
        let mut file_count = 0_u64;
        for item in archive
            .entries()
            .map_err(|_| SoftwareCenterError::Invalid)?
        {
            let mut entry = item.map_err(|_| SoftwareCenterError::Invalid)?;
            let path = entry.path().map_err(|_| SoftwareCenterError::Invalid)?;
            let mut components = path.components();
            if components.next().and_then(|part| part.as_os_str().to_str()) != Some(expected_root) {
                return Err(SoftwareCenterError::Invalid);
            }
            let relative = components.as_path();
            if relative.as_os_str().is_empty() {
                continue;
            }
            let kind = entry.header().entry_type();
            if !(kind.is_file() || kind.is_dir()) {
                return Err(SoftwareCenterError::Invalid);
            }
            file_count = file_count
                .checked_add(1)
                .ok_or(SoftwareCenterError::Invalid)?;
            expanded_bytes = expanded_bytes
                .checked_add(entry.size())
                .ok_or(SoftwareCenterError::Invalid)?;
            let relative_text = relative.to_str().ok_or(SoftwareCenterError::Invalid)?;
            self.archive_policy
                .accept(relative_text, false, expanded_bytes, file_count)
                .map_err(|_| SoftwareCenterError::Invalid)?;
            let output = destination.join(relative);
            if kind.is_dir() {
                fs::create_dir_all(&output).map_err(|_| SoftwareCenterError::Package)?;
                continue;
            }
            let parent = output.parent().ok_or(SoftwareCenterError::Invalid)?;
            fs::create_dir_all(parent).map_err(|_| SoftwareCenterError::Package)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)
                .map_err(|_| SoftwareCenterError::Package)?;
            let written =
                std::io::copy(&mut entry, &mut file).map_err(|_| SoftwareCenterError::Package)?;
            if written != entry.size() {
                return Err(SoftwareCenterError::Invalid);
            }
            file.flush().map_err(|_| SoftwareCenterError::Package)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&output, fs::Permissions::from_mode(0o644))
                    .map_err(|_| SoftwareCenterError::Package)?;
            }
        }
        Ok(())
    }
}
