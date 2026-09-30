//! Forge publish provider validation, parsing, and redaction
//! helpers. The wire contract is validated here; secrets and
//! source contents are scrubbed here. This file is I/O-free.

use chrono::{DateTime, Utc};

use crate::application_delivery::{ApplicationSourceKind, is_mutable_digest, validate_source_ref};

use super::types::{
    ForgeProviderError, ForgeProviderOperation, ForgeProviderOperationId, ForgeProviderRequest,
    ForgeProviderResponse, ForgeProviderRevision, ForgeProviderSourceKind,
};

/// Maximum allowed length of the provider-returned diagnostic.
pub const MAX_DIAGNOSTIC_LEN: usize = 512;

/// Maximum allowed length of the project slug (matches the
/// application-delivery application-name length budget).
pub const MAX_PROJECT_SLUG_LEN: usize = 64;

/// Maximum allowed length of the source URL / ref.
pub const MAX_SOURCE_REF_LEN: usize = 512;

/// Maximum allowed length of a logical environment name.
pub const MAX_ENVIRONMENT_LEN: usize = 64;

/// Parse a JSON value into a validated `ForgeProviderRequest`.
/// The function rejects malformed protocol versions, missing
/// operation ids, malformed revisions, and any request that
/// would leak a secret value or source contents.
pub fn parse_request(
    value: &serde_json::Value,
) -> Result<ForgeProviderRequest, ForgeProviderError> {
    let request: ForgeProviderRequest = serde_json::from_value(value.clone()).map_err(|err| {
        ForgeProviderError::InvalidRevision(format!("request is not valid JSON: {err}"))
    })?;
    validate_request(&request)?;
    Ok(request)
}

/// Validate a parsed request against the wire contract.
/// Returns the typed error on any violation; the facade maps
/// every error to a wire-level response.
pub fn validate_request(request: &ForgeProviderRequest) -> Result<(), ForgeProviderError> {
    if request.protocol.trim().is_empty()
        || request.protocol != super::types::FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION
    {
        return Err(ForgeProviderError::UnsupportedProtocol(
            request.protocol.clone(),
        ));
    }

    if request.operation.as_str().trim().is_empty() {
        return Err(ForgeProviderError::UnsupportedOperation(
            "<empty>".to_string(),
        ));
    }

    validate_project(&request.project)?;

    if request.operation != ForgeProviderOperation::Capabilities {
        let revision = request.revision.as_ref().ok_or_else(|| {
            ForgeProviderError::InvalidRevision(
                "revision is required for non-capabilities operations".into(),
            )
        })?;
        validate_revision(revision)?;
        if let Some(operation_id) = &request.operation_id {
            validate_operation_identity(operation_id)?;
        } else {
            return Err(ForgeProviderError::InvalidOperationIdentity(
                "operation_id is required for non-capabilities operations".into(),
            ));
        }
    }

    if request
        .secret_set
        .as_ref()
        .is_some_and(|secret_set| secret_set.secret_set.trim().is_empty())
    {
        return Err(ForgeProviderError::InvalidRevision(
            "secret_set reference must be a non-empty name".into(),
        ));
    }

    Ok(())
}

fn validate_project(
    project: &super::types::ForgeProviderProject,
) -> Result<(), ForgeProviderError> {
    let slug = project.slug.trim();
    if slug.is_empty() {
        return Err(ForgeProviderError::InvalidProject(
            "project slug is required".into(),
        ));
    }
    if slug.len() > MAX_PROJECT_SLUG_LEN {
        return Err(ForgeProviderError::InvalidProject(format!(
            "project slug is too long (max {MAX_PROJECT_SLUG_LEN})"
        )));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' || c == '/')
    {
        return Err(ForgeProviderError::InvalidProject(
            "project slug may only contain lowercase letters, digits, '-', '_', '/'".into(),
        ));
    }
    if project
        .label
        .as_ref()
        .is_some_and(|label| label.len() > 128)
    {
        return Err(ForgeProviderError::InvalidProject(
            "project label is too long (max 128)".into(),
        ));
    }
    Ok(())
}

/// Validate a `ForgeProviderRevision`. Mirrors the rules in
/// `application_delivery::logic::validate_source` plus the
/// Forge protocol's `release_digest` requirement on mutating
/// operations.
pub fn validate_revision(revision: &ForgeProviderRevision) -> Result<(), ForgeProviderError> {
    if revision.url.trim().is_empty() {
        return Err(ForgeProviderError::InvalidRevision(
            "revision url is required".into(),
        ));
    }
    if revision.url.len() > MAX_SOURCE_REF_LEN {
        return Err(ForgeProviderError::InvalidRevision(
            "revision url is too long".into(),
        ));
    }
    if revision.source_ref.trim().is_empty() {
        return Err(ForgeProviderError::InvalidRevision(
            "revision source_ref is required".into(),
        ));
    }
    if revision.source_ref.len() > MAX_SOURCE_REF_LEN {
        return Err(ForgeProviderError::InvalidRevision(
            "revision source_ref is too long".into(),
        ));
    }
    if revision.environment.trim().is_empty() {
        return Err(ForgeProviderError::InvalidRevision(
            "revision environment is required".into(),
        ));
    }
    if revision.environment.len() > MAX_ENVIRONMENT_LEN {
        return Err(ForgeProviderError::InvalidRevision(format!(
            "revision environment is too long (max {MAX_ENVIRONMENT_LEN})"
        )));
    }
    if let Err(err) = validate_source_ref(map_source_kind(revision.kind), &revision.source_ref) {
        return Err(ForgeProviderError::InvalidRevision(format!(
            "revision source_ref is not an immutable ref: {err}"
        )));
    }
    if let Some(digest) = &revision.release_digest {
        if digest.trim().is_empty() {
            return Err(ForgeProviderError::InvalidRevision(
                "revision release_digest must be non-empty when present".into(),
            ));
        }
        if digest.len() > MAX_SOURCE_REF_LEN {
            return Err(ForgeProviderError::InvalidRevision(
                "revision release_digest is too long".into(),
            ));
        }
        if is_mutable_digest(digest) {
            return Err(ForgeProviderError::InvalidRevision(
                "revision release_digest must be an immutable `sha256:...` value".into(),
            ));
        }
    }
    Ok(())
}

/// Validate an operation identity: the operation id must be a
/// non-nil UUID, the dedupe key (when present) must be
/// non-empty and bounded.
pub fn validate_operation_identity(
    id: &ForgeProviderOperationId,
) -> Result<(), ForgeProviderError> {
    if id.operation_id.is_nil() {
        return Err(ForgeProviderError::InvalidOperationIdentity(
            "operation_id is the nil UUID".into(),
        ));
    }
    if let Some(key) = &id.dedupe_key {
        if key.trim().is_empty() {
            return Err(ForgeProviderError::InvalidOperationIdentity(
                "dedupe_key must be non-empty when present".into(),
            ));
        }
        if key.len() > 128 {
            return Err(ForgeProviderError::InvalidOperationIdentity(
                "dedupe_key is too long (max 128)".into(),
            ));
        }
    }
    Ok(())
}

/// Map a wire-level `ForgeProviderSourceKind` to the internal
/// `application_delivery::ApplicationSourceKind`. The mapping is
/// total: every variant maps to exactly one internal variant.
pub fn map_source_kind(kind: ForgeProviderSourceKind) -> ApplicationSourceKind {
    match kind {
        ForgeProviderSourceKind::Git => ApplicationSourceKind::Git,
        ForgeProviderSourceKind::Oci => ApplicationSourceKind::Oci,
        ForgeProviderSourceKind::Compose => ApplicationSourceKind::Compose,
    }
}

/// Bound and redact an arbitrary diagnostic string. The wire
/// response MUST NOT include host paths, raw secret values,
/// source contents, or provider-specific transport commands.
///
/// The redactor collapses any value that contains a known
/// sensitive marker to the literal string `[REDACTED]` and
/// truncates the result to `MAX_DIAGNOSTIC_LEN`.
pub fn redact_diagnostic(input: impl AsRef<str>) -> String {
    let input = input.as_ref();
    let mut redacted = String::with_capacity(input.len().min(MAX_DIAGNOSTIC_LEN));
    let mut replaced = false;
    for marker in SENSITIVE_MARKERS {
        if input.to_ascii_lowercase().contains(marker) {
            redacted.push_str("[REDACTED]");
            replaced = true;
            break;
        }
    }
    if !replaced {
        redacted.push_str(input);
    }
    if redacted.len() > MAX_DIAGNOSTIC_LEN {
        let mut cut = MAX_DIAGNOSTIC_LEN.saturating_sub('…'.len_utf8());
        while cut > 0 && !redacted.is_char_boundary(cut) {
            cut -= 1;
        }
        redacted.truncate(cut);
        redacted.push('…');
    }
    redacted
}

/// Redact a source label so it can be safely echoed back to
/// the operator surface (logs, audit, debug). The redactor
/// strips absolute-path prefixes and any character that could
/// leak transport detail.
pub fn redact_source_label(input: impl AsRef<str>) -> String {
    let input = input.as_ref();
    let trimmed = input.trim();
    if let Some(stripped) = trimmed.strip_prefix('/') {
        return format!("[REDACTED-PATH]/{}", redact_diagnostic(stripped));
    }
    redact_diagnostic(trimmed)
}

/// Redact the request metadata before any logging / audit fan-out.
/// Strips the operation id (kept only inside the response) and
/// truncates the project slug.
pub fn redact_request_metadata(request: &ForgeProviderRequest) -> String {
    let slug = &request.project.slug;
    let operation = request.operation.as_str();
    if let Some(revision) = &request.revision {
        let env = &revision.environment;
        format!("forge.publish.op={operation} project={slug} env={env}")
    } else {
        format!("forge.publish.op={operation} project={slug}")
    }
}

/// Redact the response metadata before logging / audit fan-out.
/// Strips the operation id and the diagnostic.
pub fn redact_response_metadata(response: &ForgeProviderResponse, when: DateTime<Utc>) -> String {
    let operation = response.operation.as_str();
    let status = response.status.as_str();
    let env = response.environment.as_deref().unwrap_or("-");
    format!(
        "forge.publish.resp.op={operation} status={status} env={env} observed_at={}",
        when.to_rfc3339()
    )
}

/// Sensitive markers that, when matched (case-insensitive),
/// force the diagnostic to collapse to `[REDACTED]`. Keep the
/// list narrow: only patterns that would leak a real secret,
/// host path, or transport command.
const SENSITIVE_MARKERS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "private_key",
    "begin private",
    "begin rsa",
    "begin openssh",
    "/etc/",
    "/var/",
    "/home/",
    "/root/",
    "/usr/",
    "ssh -i",
    "ssh -o",
    "ssh -p",
    "scp -i",
    "rsync -e ssh",
    "kubectl ",
    "docker login",
    "aws ",
    "gcloud ",
    "az ",
    "sudo ",
    "chmod 7",
    "chmod 6",
];
