//! Terraform provider descriptor.

use serde::{Deserialize, Serialize};

/// Provider resource kinds shipped by the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// `openpanel_site`
    Site,
    /// `openpanel_user`
    User,
    /// `openpanel_dns_zone`
    DnsZone,
    /// `openpanel_backup`
    Backup,
}

impl ResourceKind {
    /// Stable string form (`openpanel_<kind>`).
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceKind::Site => "openpanel_site",
            ResourceKind::User => "openpanel_user",
            ResourceKind::DnsZone => "openpanel_dns_zone",
            ResourceKind::Backup => "openpanel_backup",
        }
    }

    /// Stable identifier for the backing endpoint family.
    pub fn endpoint_family(&self) -> &'static str {
        match self {
            ResourceKind::Site => "sites",
            ResourceKind::User => "identity/users",
            ResourceKind::DnsZone => "dns/zones",
            ResourceKind::Backup => "backups/plans",
        }
    }

    /// Required token scope (informational).
    pub fn required_scope(&self) -> &'static str {
        match self {
            ResourceKind::Site => "sites.write",
            ResourceKind::User => "identity.write",
            ResourceKind::DnsZone => "dns.write",
            ResourceKind::Backup => "backups.write",
        }
    }
}

/// A single provider resource declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderResource {
    /// Resource kind.
    pub kind: ResourceKind,
    /// Whether `apply` may create the resource.
    pub creatable: bool,
    /// Whether `apply` may update the resource in place.
    pub updatable: bool,
    /// Whether `destroy` is supported.
    pub destroyable: bool,
}

/// A Terraform provider contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerraformProvider {
    /// Provider name (`terraform-provider-openpanel`).
    pub name: String,
    /// Provider version (semver).
    pub version: String,
    /// OpenAPI version this provider was generated from.
    pub openapi_version: String,
    /// Resources the provider exposes.
    pub resources: Vec<ProviderResource>,
}

impl TerraformProvider {
    /// Construct a fresh provider covering the four core resources.
    pub fn core(openapi_version: impl Into<String>) -> Self {
        Self {
            name: "terraform-provider-openpanel".into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            openapi_version: openapi_version.into(),
            resources: vec![
                ProviderResource {
                    kind: ResourceKind::Site,
                    creatable: true,
                    updatable: true,
                    destroyable: true,
                },
                ProviderResource {
                    kind: ResourceKind::User,
                    creatable: true,
                    updatable: true,
                    destroyable: true,
                },
                ProviderResource {
                    kind: ResourceKind::DnsZone,
                    creatable: true,
                    updatable: true,
                    destroyable: true,
                },
                ProviderResource {
                    kind: ResourceKind::Backup,
                    creatable: true,
                    updatable: true,
                    destroyable: true,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_provider_lists_all_four_resources() {
        let p = TerraformProvider::core("1.0.0");
        assert_eq!(p.resources.len(), 4);
        let names: Vec<&'static str> = p.resources.iter().map(|r| r.kind.as_str()).collect();
        assert!(names.contains(&"openpanel_site"));
        assert!(names.contains(&"openpanel_user"));
        assert!(names.contains(&"openpanel_dns_zone"));
        assert!(names.contains(&"openpanel_backup"));
    }

    #[test]
    fn resource_kind_endpoint_family_is_stable() {
        assert_eq!(ResourceKind::Site.endpoint_family(), "sites");
        assert_eq!(ResourceKind::DnsZone.endpoint_family(), "dns/zones");
    }
}
