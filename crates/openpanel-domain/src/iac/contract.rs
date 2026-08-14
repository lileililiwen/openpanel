//! API contract descriptor and operation mapping.
//!
//! The contract is the canonical, language-neutral description of
//! what the SDK and provider expose. It is generated from
//! `openapi.rs` and checked for drift in CI.

use serde::{Deserialize, Serialize};

use super::provider::{ResourceKind, TerraformProvider};
use super::sdk::{Language, SdkPackage, SdkSurface};

/// Stable OpenAPI operation id (e.g. `sites.list`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(String);

impl OperationId {
    /// Construct from a raw `operationId`.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One entry in the `paths` table of the OpenAPI document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    /// OpenAPI operation id.
    pub id: OperationId,
    /// HTTP method (lowercase).
    pub method: String,
    /// Path template (e.g. `/sites/{id}`).
    pub path: String,
    /// Backing resource family (e.g. `sites`).
    pub family: String,
}

impl Operation {
    /// Returns `true` if the operation is a write (`post`, `put`,
    /// `patch`, `delete`).
    pub fn is_write(&self) -> bool {
        matches!(
            self.method.as_str(),
            "post" | "put" | "patch" | "delete"
        )
    }
}

/// Backing endpoint for a provider resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceEndpoint {
    /// Provider resource kind.
    pub kind: ResourceKind,
    /// OpenAPI operation id backing the create flow.
    pub create_op: OperationId,
    /// OpenAPI operation id backing the read flow.
    pub read_op: OperationId,
    /// OpenAPI operation id backing the delete flow.
    pub delete_op: OperationId,
}

/// The full API contract: every operation the SDK exposes plus
/// every provider resource and its backing endpoint set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiContract {
    /// OpenAPI version the contract was generated from.
    pub openapi_version: String,
    /// All operations the SDK may surface.
    pub operations: Vec<Operation>,
    /// Mapping from provider resource kind to its backing endpoints.
    pub resource_endpoints: Vec<ResourceEndpoint>,
}

impl ApiContract {
    /// Returns `true` if every provider resource has at least one
    /// backing endpoint.
    pub fn every_resource_backed(&self) -> bool {
        use std::collections::BTreeSet;
        let op_ids: BTreeSet<&str> = self
            .operations
            .iter()
            .map(|o| o.id.as_str())
            .collect();
        self.resource_endpoints.iter().all(|re| {
            op_ids.contains(re.create_op.as_str())
                && op_ids.contains(re.read_op.as_str())
                && op_ids.contains(re.delete_op.as_str())
        })
    }

    /// Returns the SDK surface for a given operation, picking the
    /// camelCase method name.
    pub fn surface_for(&self, op: &Operation) -> SdkSurface {
        let method_name = camel_case(&op.id.as_str());
        let resource = op.family.clone();
        SdkSurface {
            resource,
            method: op.method.clone(),
            operation_id: op.id.as_str().to_string(),
            method_name,
            required_scope: if op.is_write() {
                format!("{}.write", op.family)
            } else {
                format!("{}.read", op.family)
            },
        }
    }

    /// Build an SDK package from this contract for the given
    /// language.
    pub fn sdk_for(&self, language: Language, package_name: &str) -> SdkPackage {
        let surfaces: Vec<SdkSurface> = self
            .operations
            .iter()
            .map(|op| self.surface_for(op))
            .collect();
        SdkPackage {
            language,
            package_name: package_name.to_string(),
            openapi_version: self.openapi_version.clone(),
            surfaces,
        }
    }
}

fn camel_case(operation_id: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in operation_id.chars() {
        if ch == '.' || ch == '_' || ch == '-' {
            upper = true;
            continue;
        }
        if upper {
            for u in ch.to_uppercase() {
                out.push(u);
            }
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_operation(id: &str, method: &str, path: &str, family: &str) -> Operation {
        Operation {
            id: OperationId::new(id),
            method: method.into(),
            path: path.into(),
            family: family.into(),
        }
    }

    #[test]
    fn every_resource_backed_requires_full_set() {
        let ops = vec![
            sample_operation("sites.create", "post", "/sites", "sites"),
            sample_operation("sites.read", "get", "/sites/{id}", "sites"),
            sample_operation("sites.delete", "delete", "/sites/{id}", "sites"),
        ];
        let contract = ApiContract {
            openapi_version: "1.0.0".into(),
            operations: ops,
            resource_endpoints: vec![ResourceEndpoint {
                kind: ResourceKind::Site,
                create_op: OperationId::new("sites.create"),
                read_op: OperationId::new("sites.read"),
                delete_op: OperationId::new("sites.delete"),
            }],
        };
        assert!(contract.every_resource_backed());

        // Drop the delete op; site is no longer fully backed.
        let contract = ApiContract {
            operations: vec![
                sample_operation("sites.create", "post", "/sites", "sites"),
                sample_operation("sites.read", "get", "/sites/{id}", "sites"),
            ],
            ..contract
        };
        assert!(!contract.every_resource_backed());
    }

    #[test]
    fn surface_for_assigns_read_or_write_scope() {
        let contract = ApiContract {
            openapi_version: "1.0.0".into(),
            operations: vec![
                sample_operation("sites.list", "get", "/sites", "sites"),
                sample_operation("sites.create", "post", "/sites", "sites"),
            ],
            resource_endpoints: Vec::new(),
        };
        let read = contract.operations[0].clone();
        let write = contract.operations[1].clone();
        assert_eq!(contract.surface_for(&read).required_scope, "sites.read");
        assert_eq!(contract.surface_for(&write).required_scope, "sites.write");
    }

    #[test]
    fn sdk_for_builds_camel_case_methods() {
        let contract = ApiContract {
            openapi_version: "1.0.0".into(),
            operations: vec![
                sample_operation("sites.list", "get", "/sites", "sites"),
                sample_operation("sites.create_one", "post", "/sites", "sites"),
            ],
            resource_endpoints: Vec::new(),
        };
        let sdk = contract.sdk_for(Language::Rust, "openpanel-sdk");
        assert_eq!(sdk.surfaces[0].method_name, "SitesList");
        assert_eq!(sdk.surfaces[1].method_name, "SitesCreateOne");
    }

    #[test]
    fn camel_case_works_on_kebab_case() {
        assert_eq!(camel_case("sites-list"), "SitesList");
        assert_eq!(camel_case("dns_zone.read"), "DnsZoneRead");
    }
}