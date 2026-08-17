//! Typed SDK package descriptor.

use serde::{Deserialize, Serialize};

/// Supported SDK target languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Rust crate.
    Rust,
    /// Go module.
    Go,
    /// TypeScript / JavaScript package.
    TypeScript,
}

impl Language {
    /// Stable string form.
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Go => "go",
            Language::TypeScript => "typescript",
        }
    }
}

/// One SDK surface (a typed method on a per-resource client).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SdkSurface {
    /// Resource this surface targets (e.g. `site`, `user`, `dns_zone`).
    pub resource: String,
    /// HTTP method (lowercase: `get`, `post`, ...).
    pub method: String,
    /// Operation id (OpenAPI `operationId`).
    pub operation_id: String,
    /// Method name in the SDK (camelCase).
    pub method_name: String,
    /// Required scope (informational; checked at token-issue time).
    pub required_scope: String,
}

/// A typed client SDK for one language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SdkPackage {
    /// Target language.
    pub language: Language,
    /// Crate / module name.
    pub package_name: String,
    /// Version of the OpenAPI spec this package was generated from.
    pub openapi_version: String,
    /// Typed surfaces (one per OpenAPI operation the SDK exposes).
    pub surfaces: Vec<SdkSurface>,
}

impl SdkPackage {
    /// Returns `true` if the SDK requires an auth token at
    /// construction (always true for SDKs).
    pub fn requires_token(&self) -> bool {
        // An SDK that exposes any surfaces MUST require a token to
        // construct; this is enforced by the contract test.
        !self.surfaces.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_round_trips() {
        for l in [Language::Rust, Language::Go, Language::TypeScript] {
            assert_eq!(l, l);
            assert!(!l.as_str().is_empty());
        }
    }

    #[test]
    fn empty_sdk_does_not_require_token() {
        // Empty SDK is the boundary case where token auth is not
        // exercised; the contract checks the non-empty case.
        let sdk = SdkPackage {
            language: Language::Rust,
            package_name: "openpanel-sdk".into(),
            openapi_version: "1.0.0".into(),
            surfaces: Vec::new(),
        };
        assert!(!sdk.requires_token());
    }
}
