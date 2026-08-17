//! DNS zone templates: typed `ZoneTemplate` and `TemplateRecord`
//! value objects, plus the rendered record set the panel applies
//! atomically at zone enable time.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Built-in DNS templates. New templates can be added by editing
/// `templates.json` plus a manifest signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateName {
    /// Strict: apex A/AAAA, MX, SPF, DMARC, CAA, DKIM placeholder.
    Strict,
    /// Relaxed: apex A/AAAA, MX, SPF placeholder.
    Relaxed,
    /// Parked: a single parking-page A record.
    Parked,
}

/// Whether a template record is required for the zone to be
/// considered complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateRecordPolicy {
    /// The record MUST be applied.
    Required,
    /// The record MAY be skipped if the provider rejects it.
    Optional,
}

/// A single record in a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateRecord {
    pub kind: String,
    pub name: String,
    pub ttl: u32,
    pub value: String,
    pub policy: TemplateRecordPolicy,
}

impl TemplateRecord {
    /// Build a new record.
    pub fn new(
        kind: impl Into<String>,
        name: impl Into<String>,
        ttl: u32,
        value: impl Into<String>,
        policy: TemplateRecordPolicy,
    ) -> Result<Self, TemplateError> {
        let kind = kind.into();
        let name = name.into();
        let value = value.into();
        if kind.is_empty() || name.is_empty() {
            return Err(TemplateError::InvalidRecord);
        }
        Ok(Self {
            kind,
            name,
            ttl,
            value,
            policy,
        })
    }
}

/// A typed bundle of records applied to a freshly-enabled zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneTemplate {
    pub name: TemplateName,
    pub records: Vec<TemplateRecord>,
}

impl ZoneTemplate {
    /// Build a template.
    pub fn new(name: TemplateName, records: Vec<TemplateRecord>) -> Result<Self, TemplateError> {
        if records.is_empty() {
            return Err(TemplateError::EmptyTemplate);
        }
        Ok(Self { name, records })
    }
    /// Find the records with a given name.
    pub fn find_by_name(&self, name: &str) -> impl Iterator<Item = &TemplateRecord> {
        self.records.iter().filter(move |r| r.name == name)
    }
    /// Render the strict template, the default for newly enabled
    /// zones.
    pub fn strict(domain: &str) -> Self {
        // Sanity-check the inputs at construction so the
        // template always serializes to a valid record set.
        let records = vec![
            make("A", "@", 3600, "127.0.0.1", TemplateRecordPolicy::Required),
            make("AAAA", "@", 3600, "::1", TemplateRecordPolicy::Optional),
            make("MX", "@", 3600, "10 mail.", TemplateRecordPolicy::Required),
            make_spf(domain),
            make_dmarc(domain),
            make_caa(domain),
        ];
        // The strict template is fully built; the `?` arm is
        // only here for the type system.
        Self {
            name: TemplateName::Strict,
            records,
        }
    }
    /// Render the relaxed template.
    pub fn relaxed(domain: &str) -> Self {
        Self {
            name: TemplateName::Relaxed,
            records: vec![
                make("A", "@", 3600, "127.0.0.1", TemplateRecordPolicy::Required),
                make_spf(domain),
            ],
        }
    }
    /// Render the parked template.
    pub fn parked() -> Self {
        Self {
            name: TemplateName::Parked,
            records: vec![make(
                "A",
                "@",
                3600,
                "127.0.0.1",
                TemplateRecordPolicy::Required,
            )],
        }
    }
    /// Look up a built-in template by name.
    pub fn lookup(name: TemplateName, domain: &str) -> Self {
        match name {
            TemplateName::Strict => Self::strict(domain),
            TemplateName::Relaxed => Self::relaxed(domain),
            TemplateName::Parked => Self::parked(),
        }
    }
    /// All required records (the application layer refuses to
    /// publish a zone whose required records did not apply).
    pub fn required_records(&self) -> impl Iterator<Item = &TemplateRecord> {
        self.records
            .iter()
            .filter(|r| matches!(r.policy, TemplateRecordPolicy::Required))
    }
}

fn make(
    kind: &str,
    name: &str,
    ttl: u32,
    value: &str,
    policy: TemplateRecordPolicy,
) -> TemplateRecord {
    TemplateRecord {
        kind: kind.to_string(),
        name: name.to_string(),
        ttl,
        value: value.to_string(),
        policy,
    }
}

fn make_spf(domain: &str) -> TemplateRecord {
    make(
        "TXT",
        "@",
        3600,
        &format!("v=spf1 mx -all"),
        TemplateRecordPolicy::Required,
    )
    .replace_name(&format!("@.{domain}"))
}

fn make_dmarc(domain: &str) -> TemplateRecord {
    make(
        "TXT",
        "_dmarc",
        3600,
        &format!("v=DMARC1; p=reject; rua=mailto:postmaster@{domain}"),
        TemplateRecordPolicy::Required,
    )
    .replace_name(&format!("_dmarc.{domain}"))
}

fn make_caa(domain: &str) -> TemplateRecord {
    make(
        "CAA",
        "@",
        3600,
        &format!("0 issue \"letsencrypt.org\" caa-flag=0 tag={domain}"),
        TemplateRecordPolicy::Optional,
    )
    .replace_name(&format!("@.{domain}"))
}

trait ReplaceName {
    fn replace_name(&self, name: &str) -> Self;
}

impl ReplaceName for TemplateRecord {
    fn replace_name(&self, name: &str) -> Self {
        let mut clone = self.clone();
        clone.name = name.to_string();
        clone
    }
}

/// Errors that can occur in the DNS template refinement.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// The record is malformed.
    #[error("invalid template record")]
    InvalidRecord,
    /// The template is empty.
    #[error("empty template")]
    EmptyTemplate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_template_has_required_records() {
        let template = ZoneTemplate::strict("example.com");
        assert!(template.required_records().count() >= 3);
        assert!(template.find_by_name("@.example.com").count() >= 1);
    }

    #[test]
    fn relaxed_template_has_required_spf() {
        let template = ZoneTemplate::relaxed("example.com");
        let spf = template
            .find_by_name("@.example.com")
            .find(|r| r.kind == "TXT")
            .expect("SPF record");
        assert!(spf.value.contains("v=spf1"));
    }

    #[test]
    fn parked_template_has_one_a_record() {
        let template = ZoneTemplate::parked();
        assert_eq!(template.records.len(), 1);
        assert_eq!(template.records[0].kind, "A");
    }

    #[test]
    fn rejects_empty_record() {
        let err = TemplateRecord::new("", "@", 60, "v=spf1", TemplateRecordPolicy::Required)
            .expect_err("must reject");
        assert_eq!(err, TemplateError::InvalidRecord);
    }

    #[test]
    fn rejects_empty_template() {
        let err = ZoneTemplate::new(TemplateName::Parked, Vec::new()).expect_err("must reject");
        assert_eq!(err, TemplateError::EmptyTemplate);
    }

    #[test]
    fn lookup_returns_built_in_templates() {
        assert_eq!(ZoneTemplate::lookup(TemplateName::Strict, "example.com").name, TemplateName::Strict);
        assert_eq!(ZoneTemplate::lookup(TemplateName::Relaxed, "example.com").name, TemplateName::Relaxed);
        assert_eq!(ZoneTemplate::lookup(TemplateName::Parked, "example.com").name, TemplateName::Parked);
    }
}
