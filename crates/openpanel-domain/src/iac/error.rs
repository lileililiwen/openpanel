//! IaC contract errors.

use thiserror::Error;

/// Errors raised by the IaC bounded context.
#[derive(Debug, Error)]
pub enum IacError {
    /// The OpenAPI document could not be parsed.
    #[error("openapi parse error: {0}")]
    OpenApiParse(String),
    /// A required field is missing in the OpenAPI document.
    #[error("openapi missing field: {0}")]
    OpenApiMissing(String),
    /// A provider resource is missing the backing endpoint.
    #[error("resource {0} is not backed by any endpoint in the contract")]
    UnbackedResource(String),
    /// An SDK surface is missing the required `auth: Token` field.
    #[error("sdk surface {0} missing auth token")]
    UnauthenticatedSdk(String),
    /// The drift check failed: generated and committed differ.
    #[error("drift detected in {0}: {1} bytes differ")]
    Drift(String, usize),
    /// Generic I/O failure.
    #[error("io error: {0}")]
    Io(String),
}
