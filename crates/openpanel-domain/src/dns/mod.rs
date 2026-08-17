//! Provider-neutral DNS names, records, capabilities, and invariants.

use std::{
    fmt,
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// Re-export the zone template refinement types from the
// `refine-dns-with-zone-templates` change so callers can import them
// from the `dns` module without crossing module boundaries.
pub use templates::{TemplateError, TemplateName, TemplateRecord, TemplateRecordPolicy, ZoneTemplate};

mod templates;

/// DNS validation failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DnsError {
    /// A name, value, version, or bound is invalid.
    #[error("invalid DNS value")]
    Invalid,
    /// A CNAME shares an owner name with another record.
    #[error("CNAME must be the only record at its name")]
    CnameConflict,
    /// The provider does not advertise this operation.
    #[error("DNS provider capability is unavailable")]
    Unsupported,
}

/// Canonical absolute DNS name without a trailing dot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DnsName(String);
impl DnsName {
    /// Parse, lower-case, and validate a DNS name. Labels may contain
    /// only ASCII letters, digits, and hyphens, matching RFC 1035 host
    /// syntax. Names that need to carry leading underscores (e.g. the
    /// `_acme-challenge` TXT labels used for ACME HTTP-01 / DNS-01
    /// challenges) MUST go through [`DnsName::new_with_underscore`].
    pub fn new(value: impl AsRef<str>) -> Result<Self, DnsError> {
        Self::parse(value, false)
    }

    /// Parse a DNS name that is allowed to carry leading underscores in
    /// any of its labels. Reserved for record kinds that the protocol
    /// deliberately prefixes with `_` (ACME challenges, DKIM, DMARC,
    /// etc.); do not use for hostnames resolved by A/AAAA/CNAME/MX/NS.
    pub fn new_with_underscore(value: impl AsRef<str>) -> Result<Self, DnsError> {
        Self::parse(value, true)
    }

    fn parse(value: impl AsRef<str>, allow_underscore: bool) -> Result<Self, DnsError> {
        let value = value
            .as_ref()
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if value.is_empty() || value.len() > 253 || value.contains('*') {
            return Err(DnsError::Invalid);
        }
        let valid = value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|ch| {
                    ch.is_ascii_lowercase()
                        || ch.is_ascii_digit()
                        || ch == '-'
                        || (allow_underscore && ch == '_')
                })
        });
        if !valid {
            return Err(DnsError::Invalid);
        }
        Ok(Self(value))
    }

    /// Canonical name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for DnsName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Validated record TTL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ttl(u32);
impl Ttl {
    /// Validate a TTL against provider bounds.
    pub fn new(value: u32, minimum: u32, maximum: u32) -> Result<Self, DnsError> {
        if minimum == 0 || minimum > maximum || value < minimum || value > maximum {
            return Err(DnsError::Invalid);
        }
        Ok(Self(value))
    }

    /// TTL seconds.
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Supported provider-neutral record kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RecordKind {
    /// IPv4 address.
    A,
    /// IPv6 address.
    Aaaa,
    /// Canonical-name alias.
    Cname,
    /// Text value.
    Txt,
    /// Mail exchanger.
    Mx,
    /// Certification authority authorization.
    Caa,
    /// Authoritative nameserver.
    Ns,
    /// Service location.
    Srv,
}

/// Typed, normalized record payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "UPPERCASE")]
pub enum RecordData {
    /// IPv4 address.
    A(Ipv4Addr),
    /// IPv6 address.
    Aaaa(Ipv6Addr),
    /// Canonical target name.
    Cname(DnsName),
    /// One logical TXT value.
    Txt(String),
    /// Preference and mail exchanger.
    Mx {
        /// Lower values have higher preference.
        priority: u16,
        /// Mail exchanger host.
        exchange: DnsName,
    },
    /// Flags, tag, and authority value.
    Caa {
        /// CAA flags byte.
        flags: u8,
        /// Standard CAA tag.
        tag: String,
        /// Authority value.
        value: String,
    },
    /// Nameserver name.
    Ns(DnsName),
    /// Priority, weight, port, and service target.
    Srv {
        /// Service priority.
        priority: u16,
        /// Relative selection weight.
        weight: u16,
        /// Service port.
        port: u16,
        /// Service host.
        target: DnsName,
    },
}
impl RecordData {
    /// Parse a presentation-format value for a known kind.
    pub fn parse(kind: RecordKind, value: &str) -> Result<Self, DnsError> {
        let value = value.trim();
        match kind {
            RecordKind::A => Ipv4Addr::from_str(value)
                .map(Self::A)
                .map_err(|_| DnsError::Invalid),
            RecordKind::Aaaa => Ipv6Addr::from_str(value)
                .map(Self::Aaaa)
                .map_err(|_| DnsError::Invalid),
            RecordKind::Cname => DnsName::new(value).map(Self::Cname),
            RecordKind::Txt
                if !value.is_empty()
                    && value.len() <= 2048
                    && !value.chars().any(char::is_control) =>
            {
                Ok(Self::Txt(value.to_owned()))
            }
            RecordKind::Mx => {
                let mut fields = value.split_whitespace();
                let priority = parse_next::<u16>(&mut fields)?;
                let exchange = DnsName::new(fields.next().ok_or(DnsError::Invalid)?)?;
                require_end(fields)?;
                Ok(Self::Mx { priority, exchange })
            }
            RecordKind::Caa => {
                let mut fields = value.splitn(3, ' ');
                let flags = fields
                    .next()
                    .ok_or(DnsError::Invalid)?
                    .parse()
                    .map_err(|_| DnsError::Invalid)?;
                let tag = fields.next().ok_or(DnsError::Invalid)?.to_ascii_lowercase();
                let value = fields.next().ok_or(DnsError::Invalid)?.trim().to_owned();
                if !matches!(tag.as_str(), "issue" | "issuewild" | "iodef")
                    || value.is_empty()
                    || value.chars().any(char::is_control)
                {
                    return Err(DnsError::Invalid);
                }
                Ok(Self::Caa { flags, tag, value })
            }
            RecordKind::Ns => DnsName::new(value).map(Self::Ns),
            RecordKind::Srv => {
                let mut fields = value.split_whitespace();
                let priority = parse_next::<u16>(&mut fields)?;
                let weight = parse_next::<u16>(&mut fields)?;
                let port = parse_next::<u16>(&mut fields)?;
                let target = DnsName::new(fields.next().ok_or(DnsError::Invalid)?)?;
                require_end(fields)?;
                Ok(Self::Srv {
                    priority,
                    weight,
                    port,
                    target,
                })
            }
            RecordKind::Txt => Err(DnsError::Invalid),
        }
    }

    /// Record kind.
    pub fn kind(&self) -> RecordKind {
        match self {
            Self::A(_) => RecordKind::A,
            Self::Aaaa(_) => RecordKind::Aaaa,
            Self::Cname(_) => RecordKind::Cname,
            Self::Txt(_) => RecordKind::Txt,
            Self::Mx { .. } => RecordKind::Mx,
            Self::Caa { .. } => RecordKind::Caa,
            Self::Ns(_) => RecordKind::Ns,
            Self::Srv { .. } => RecordKind::Srv,
        }
    }
}
impl fmt::Display for RecordData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::A(value) => write!(formatter, "{value}"),
            Self::Aaaa(value) => write!(formatter, "{value}"),
            Self::Cname(value) => write!(formatter, "{value}"),
            Self::Txt(value) => formatter.write_str(value),
            Self::Mx { priority, exchange } => write!(formatter, "{priority} {exchange}"),
            Self::Caa { flags, tag, value } => write!(formatter, "{flags} {tag} {value}"),
            Self::Ns(value) => write!(formatter, "{value}"),
            Self::Srv {
                priority,
                weight,
                port,
                target,
            } => write!(formatter, "{priority} {weight} {port} {target}"),
        }
    }
}

fn parse_next<'a, T: FromStr>(fields: &mut impl Iterator<Item = &'a str>) -> Result<T, DnsError> {
    fields
        .next()
        .ok_or(DnsError::Invalid)?
        .parse()
        .map_err(|_| DnsError::Invalid)
}
fn require_end<'a>(mut fields: impl Iterator<Item = &'a str>) -> Result<(), DnsError> {
    if fields.next().is_some() {
        Err(DnsError::Invalid)
    } else {
        Ok(())
    }
}

/// Opaque provider concurrency token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteVersion(String);
impl RemoteVersion {
    /// Validate an opaque, bounded, printable token.
    pub fn new(value: impl Into<String>) -> Result<Self, DnsError> {
        let value = value.into();
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(DnsError::Invalid);
        }
        Ok(Self(value))
    }

    /// Provider token.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Locally synchronized remote record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsRecord {
    /// Local stable ID.
    pub id: Uuid,
    /// Owner name.
    pub name: DnsName,
    /// Typed value.
    pub data: RecordData,
    /// TTL.
    pub ttl: Ttl,
    /// Provider record ID.
    pub remote_id: String,
    /// Last observed provider version.
    pub remote_version: RemoteVersion,
}
impl DnsRecord {
    /// Construct from already validated components.
    pub fn new(
        id: Uuid,
        name: DnsName,
        data: RecordData,
        ttl: Ttl,
        remote_id: impl Into<String>,
        remote_version: RemoteVersion,
    ) -> Self {
        Self {
            id,
            name,
            data,
            ttl,
            remote_id: remote_id.into(),
            remote_version,
        }
    }
}

/// Enforce RFC CNAME owner-name exclusivity.
pub fn validate_record_set(records: &[DnsRecord]) -> Result<(), DnsError> {
    for record in records {
        let same_name = records
            .iter()
            .filter(|other| other.name == record.name)
            .count();
        if same_name > 1
            && (record.data.kind() == RecordKind::Cname
                || records.iter().any(|other| {
                    other.name == record.name && other.data.kind() == RecordKind::Cname
                }))
        {
            return Err(DnsError::CnameConflict);
        }
    }
    Ok(())
}

/// Provider-discovered record and TTL support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Supported record kinds.
    pub record_kinds: Vec<RecordKind>,
    /// Minimum TTL.
    pub minimum_ttl: u32,
    /// Maximum TTL.
    pub maximum_ttl: u32,
    /// Whether provider proxying is supported.
    pub proxy: bool,
}
impl ProviderCapabilities {
    /// Capabilities supporting every v1 kind.
    pub fn all(minimum_ttl: u32, maximum_ttl: u32) -> Self {
        Self {
            record_kinds: vec![
                RecordKind::A,
                RecordKind::Aaaa,
                RecordKind::Cname,
                RecordKind::Txt,
                RecordKind::Mx,
                RecordKind::Caa,
                RecordKind::Ns,
                RecordKind::Srv,
            ],
            minimum_ttl,
            maximum_ttl,
            proxy: false,
        }
    }

    /// Check record kind support.
    pub fn supports(&self, kind: RecordKind) -> bool {
        self.record_kinds.contains(&kind)
    }
}
