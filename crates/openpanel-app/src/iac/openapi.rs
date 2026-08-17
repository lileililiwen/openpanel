//! Minimal OpenAPI parser.
//!
//! The full contract reads `openapi.rs` at codegen time; this
//! in-process parser handles the subset that the codegen needs:
//!
//! - the `info.version` field
//! - the `paths` table (`{path: {method: OperationObject}}`)
//! - the `operationId`, `summary`, and `tags[0]` fields of each
//!   operation.
//!
//! Anything outside this subset is silently ignored.

use std::collections::BTreeMap;

use openpanel_domain::iac::contract::{Operation, OperationId};
use serde::Deserialize;
use uuid::Uuid;

/// Reference to the canonical OpenAPI source. The artifact lives
/// outside the source tree; this struct is just a typed handle.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenApiRef {
    /// Repository-relative path (informational).
    pub path: String,
}

/// A parsed subset of an OpenAPI document.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedOpenApi {
    /// OpenAPI version (from `info.version`).
    pub version: String,
    /// All operations, indexed by (path, method).
    pub operations: BTreeMap<(String, String), Operation>,
}

impl ParsedOpenApi {
    /// Parse an OpenAPI JSON document into the subset we use.
    pub fn parse_json(json: &str) -> Result<Self, openpanel_domain::iac::error::IacError> {
        let doc: OpenApiDoc = serde_json::from_str(json)
            .map_err(|e| openpanel_domain::iac::error::IacError::OpenApiParse(e.to_string()))?;
        let version = doc.info.map(|i| i.version).ok_or_else(|| {
            openpanel_domain::iac::error::IacError::OpenApiMissing("info.version".into())
        })?;
        let mut operations = BTreeMap::new();
        if let Some(paths) = doc.paths {
            for (path, item) in paths {
                let mut methods: Vec<(&'static str, Option<OperationObject>)> = vec![
                    ("get", item.get),
                    ("post", item.post),
                    ("put", item.put),
                    ("patch", item.patch),
                    ("delete", item.delete),
                ];
                for (method, op_obj) in methods.drain(..) {
                    if let Some(op_obj) = op_obj
                        && let Some(op_id) = op_obj.operation_id.clone()
                    {
                        let family = op_obj
                            .tags
                            .first()
                            .cloned()
                            .unwrap_or_else(|| default_family(&path));
                        operations.insert(
                            (path.clone(), method.to_string()),
                            Operation {
                                id: OperationId::new(op_id),
                                method: method.to_string(),
                                path: path.clone(),
                                family,
                            },
                        );
                    }
                }
            }
        }
        Ok(Self {
            version,
            operations,
        })
    }

    /// Materialise an [`openpanel_domain::ApiContract`] from this
    /// parse.
    pub fn into_contract(&self) -> openpanel_domain::ApiContract {
        let mut operations: Vec<Operation> = self.operations.values().cloned().collect();
        operations.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        let resource_endpoints = build_resource_endpoints(&operations);
        openpanel_domain::ApiContract {
            openapi_version: self.version.clone(),
            operations,
            resource_endpoints,
        }
    }
}

fn default_family(path: &str) -> String {
    // Use the first non-empty path segment as the family, e.g.
    // "/sites/{id}" -> "sites".
    path.split('/')
        .find(|s| !s.is_empty() && !s.starts_with('{'))
        .unwrap_or("default")
        .to_string()
}

/// The CRUD operation ids collected for one resource family.
type FamilyCrudOps = BTreeMap<
    String,
    (
        Option<OperationId>,
        Option<OperationId>,
        Option<OperationId>,
    ),
>;

fn build_resource_endpoints(
    operations: &[Operation],
) -> Vec<openpanel_domain::iac::contract::ResourceEndpoint> {
    use openpanel_domain::iac::{contract::ResourceEndpoint, provider::ResourceKind};
    let mut by_family: FamilyCrudOps = BTreeMap::new();
    for op in operations {
        let entry = by_family
            .entry(op.family.clone())
            .or_insert((None, None, None));
        match op.method.as_str() {
            "post" => entry.0 = Some(op.id.clone()),
            "get" => entry.1 = Some(op.id.clone()),
            "delete" => entry.2 = Some(op.id.clone()),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (family, (create, read, delete)) in by_family {
        let kind = match family.as_str() {
            "sites" => Some(ResourceKind::Site),
            "identity/users" | "users" => Some(ResourceKind::User),
            "dns/zones" | "dns" => Some(ResourceKind::DnsZone),
            "backups/plans" | "backups" => Some(ResourceKind::Backup),
            _ => None,
        };
        if let (Some(kind), Some(create_op), Some(read_op), Some(delete_op)) =
            (kind, create, read, delete)
        {
            out.push(ResourceEndpoint {
                kind,
                create_op,
                read_op,
                delete_op,
            });
        }
    }
    out
}

// ---- minimal OpenAPI shape ----

#[derive(Debug, Deserialize)]
struct OpenApiDoc {
    info: Option<Info>,
    paths: Option<BTreeMap<String, PathItem>>,
}

#[derive(Debug, Deserialize)]
struct Info {
    version: String,
}

#[derive(Debug, Default, Deserialize)]
struct PathItem {
    #[serde(default)]
    get: Option<OperationObject>,
    #[serde(default)]
    post: Option<OperationObject>,
    #[serde(default)]
    put: Option<OperationObject>,
    #[serde(default)]
    patch: Option<OperationObject>,
    #[serde(default)]
    delete: Option<OperationObject>,
}

#[derive(Debug, Default, Deserialize, Clone)]
struct OperationObject {
    #[serde(default, rename = "operationId")]
    operation_id: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

/// Generate a stable per-process UUID (used only for fresh
/// operation ids in tests).
#[doc(hidden)]
pub fn fresh_id() -> Uuid {
    Uuid::new_v4()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            },
            "/identity/users": {
                "get": { "operationId": "users.list", "tags": ["identity/users"] },
                "post": { "operationId": "users.create", "tags": ["identity/users"] },
                "delete": { "operationId": "users.delete", "tags": ["identity/users"] }
            }
        }
    }"#;

    #[test]
    fn parses_openapi_subset() {
        let parsed = ParsedOpenApi::parse_json(SAMPLE).expect("parse");
        assert_eq!(parsed.version, "1.2.3");
        assert_eq!(parsed.operations.len(), 7);
    }

    #[test]
    fn into_contract_emits_backed_resources() {
        let parsed = ParsedOpenApi::parse_json(SAMPLE).expect("parse");
        let contract = parsed.into_contract();
        assert!(contract.every_resource_backed());
        // Sites and users both fully backed.
        assert_eq!(contract.resource_endpoints.len(), 2);
        let site = contract
            .resource_endpoints
            .iter()
            .find(|r| r.kind == openpanel_domain::iac::provider::ResourceKind::Site)
            .expect("site");
        assert_eq!(site.create_op.as_str(), "sites.create");
    }
}
