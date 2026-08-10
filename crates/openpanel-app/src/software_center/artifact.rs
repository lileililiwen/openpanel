//! Checksummed, resource-bounded extraction for pinned official application archives.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use flate2::read::GzDecoder;
use futures_util::StreamExt;
use openpanel_domain::software_center::ArchivePolicy;
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
        if !artifact.digest.verify(&bytes) {
            return Err(SoftwareCenterError::Invalid);
        }
        Ok(bytes)
    }
}

fn validate_artifact_url(url: &str) -> Result<(), SoftwareCenterError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| SoftwareCenterError::Invalid)?;
    let host = parsed.host_str().ok_or(SoftwareCenterError::Invalid)?;
    if parsed.scheme() != "https"
        || !matches!(
            host,
            "wordpress.org" | "downloads.wordpress.org" | "ftp.drupal.org"
        )
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
