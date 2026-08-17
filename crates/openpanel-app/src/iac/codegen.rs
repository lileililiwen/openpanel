//! Codegen contract: reconciles the SDK and Terraform provider
//! descriptors against `openapi.rs` and detects drift between
//! committed and freshly-generated artifacts.

use openpanel_domain::iac::{
    IacError as DomainIacError,
    contract::ApiContract,
    error::IacError,
    provider::TerraformProvider,
    sdk::{Language, SdkPackage},
};

/// Outcome of a drift check.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DriftOutcome {
    /// `true` when generated and committed artefacts match.
    pub clean: bool,
    /// Bytes that differ (informational; the actual diff is left to
    /// the caller).
    pub diff_bytes: usize,
}

/// Codegen contract checker. Holds the on-disk artefacts the
/// panel treats as canonical and produces fresh artefacts from a
/// parsed `ApiContract` for comparison.
#[derive(Debug, Clone)]
pub struct CodegenContract {
    /// OpenAPI version the contract expects.
    pub openapi_version: String,
    /// Crate name of the generated Rust SDK.
    pub rust_sdk_name: String,
    /// Module path of the generated Go SDK.
    pub go_module: String,
    /// Package name of the generated TypeScript SDK.
    pub ts_package_name: String,
    /// Provider name.
    pub provider_name: String,
}

impl CodegenContract {
    /// Construct a new contract from an OpenAPI version string.
    pub fn new(openapi_version: impl Into<String>) -> Self {
        Self {
            openapi_version: openapi_version.into(),
            rust_sdk_name: "openpanel-sdk".into(),
            go_module: "github.com/openpanel/openpanel-sdk-go".into(),
            ts_package_name: "@openpanel/sdk".into(),
            provider_name: "terraform-provider-openpanel".into(),
        }
    }

    /// Build the three SDK packages + the Terraform provider from
    /// a parsed `ApiContract`. The result is the freshly-generated
    /// surface.
    pub fn build(&self, contract: &ApiContract) -> GeneratedSurface {
        let rust = contract.sdk_for(Language::Rust, &self.rust_sdk_name);
        let go = contract.sdk_for(Language::Go, &self.go_module);
        let ts = contract.sdk_for(Language::TypeScript, &self.ts_package_name);
        let provider = TerraformProvider {
            name: self.provider_name.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            openapi_version: self.openapi_version.clone(),
            resources: contract
                .resource_endpoints
                .iter()
                .map(|re| openpanel_domain::iac::provider::ProviderResource {
                    kind: re.kind,
                    creatable: true,
                    updatable: true,
                    destroyable: true,
                })
                .collect(),
        };
        GeneratedSurface {
            rust,
            go,
            ts,
            provider,
        }
    }

    /// Run the drift check between the generated surface and the
    /// on-disk canonical artefacts. The on-disk artefacts are
    /// passed in as raw bytes; the caller is responsible for
    /// reading them from the repository (so this module stays
    /// filesystem-agnostic and unit-testable).
    pub fn drift(
        &self,
        contract: &ApiContract,
        committed: &CommittedArtifacts,
    ) -> Result<DriftOutcome, IacError> {
        let generated = self.build(contract);
        let generated_provider_json = serde_json::to_string_pretty(&generated.provider)
            .map_err(|e| DomainIacError::OpenApiParse(e.to_string()))?;
        let generated_rust_json = serde_json::to_string_pretty(&generated.rust)
            .map_err(|e| DomainIacError::OpenApiParse(e.to_string()))?;
        if generated_provider_json != committed.provider_json {
            let diff = first_diff(&generated_provider_json, &committed.provider_json);
            return Ok(DriftOutcome {
                clean: false,
                diff_bytes: diff,
            });
        }
        if generated_rust_json != committed.rust_sdk_json {
            let diff = first_diff(&generated_rust_json, &committed.rust_sdk_json);
            return Ok(DriftOutcome {
                clean: false,
                diff_bytes: diff,
            });
        }
        Ok(DriftOutcome {
            clean: true,
            diff_bytes: 0,
        })
    }
}

/// The freshly-generated artefacts.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedSurface {
    /// Rust SDK package.
    pub rust: SdkPackage,
    /// Go SDK package.
    pub go: SdkPackage,
    /// TypeScript SDK package.
    pub ts: SdkPackage,
    /// Terraform provider.
    pub provider: TerraformProvider,
}

/// On-disk canonical artefacts loaded by the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct CommittedArtifacts {
    /// JSON-serialised committed Rust SDK.
    pub rust_sdk_json: String,
    /// JSON-serialised committed Terraform provider.
    pub provider_json: String,
}

fn first_diff(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut diff = 0;
    for i in 0..a.len().min(b.len()) {
        if a[i] != b[i] {
            diff += 1;
        }
    }
    diff + (a.len() as isize - b.len() as isize).unsigned_abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iac::openapi::ParsedOpenApi;

    const SAMPLE: &str = r#"{
        "openapi": "3.0.0",
        "info": { "version": "1.2.3", "title": "OpenPanel" },
        "paths": {
            "/sites": {
                "get": { "operationId": "sites.list", "tags": ["sites"] },
                "post": { "operationId": "sites.create", "tags": ["sites"] }
            },
            "/sites/{id}": {
                "get": { "operationId": "sites.read", "tags": ["sites"] },
                "delete": { "operationId": "sites.delete", "tags": ["sites"] }
            }
        }
    }"#;

    fn build_contract() -> ApiContract {
        ParsedOpenApi::parse_json(SAMPLE).unwrap().into_contract()
    }

    #[test]
    fn build_emits_three_sdks_and_a_provider() {
        let contract = build_contract();
        let codegen = CodegenContract::new("1.2.3");
        let surface = codegen.build(&contract);
        assert_eq!(surface.rust.surfaces.len(), 4);
        assert_eq!(surface.go.surfaces.len(), 4);
        assert_eq!(surface.ts.surfaces.len(), 4);
        assert_eq!(surface.provider.resources.len(), 1);
        assert_eq!(
            surface.provider.resources[0].kind,
            openpanel_domain::iac::provider::ResourceKind::Site
        );
    }

    #[test]
    fn drift_clean_when_committed_matches_generated() {
        let contract = build_contract();
        let codegen = CodegenContract::new("1.2.3");
        let surface = codegen.build(&contract);
        let committed = CommittedArtifacts {
            rust_sdk_json: serde_json::to_string_pretty(&surface.rust).unwrap(),
            provider_json: serde_json::to_string_pretty(&surface.provider).unwrap(),
        };
        let outcome = codegen.drift(&contract, &committed).unwrap();
        assert!(outcome.clean);
    }

    #[test]
    fn drift_detects_when_committed_drifts() {
        let contract = build_contract();
        let codegen = CodegenContract::new("1.2.3");
        let surface = codegen.build(&contract);
        let mut provider = surface.provider.clone();
        provider.version = "drifted".into();
        let committed = CommittedArtifacts {
            rust_sdk_json: serde_json::to_string_pretty(&surface.rust).unwrap(),
            provider_json: serde_json::to_string_pretty(&provider).unwrap(),
        };
        let outcome = codegen.drift(&contract, &committed).unwrap();
        assert!(!outcome.clean);
        assert!(outcome.diff_bytes > 0);
    }
}
