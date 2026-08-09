//! Bounded DNS propagation resolver port and DNS-over-HTTPS adapter.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use super::{DnsServiceError, RemoteRecord};

/// Propagation observation port.
#[async_trait]
pub trait DnsResolver: Send + Sync {
    /// Whether a public resolver currently observes the synchronized value.
    async fn observes(&self, record: &RemoteRecord) -> Result<bool, DnsServiceError>;
}

/// Deterministic resolver used only in isolated service tests.
pub(crate) struct SynchronizedResolver;
#[async_trait]
impl DnsResolver for SynchronizedResolver {
    async fn observes(&self, _record: &RemoteRecord) -> Result<bool, DnsServiceError> {
        Ok(true)
    }
}

/// Cloudflare JSON DNS-over-HTTPS propagation adapter.
pub struct CloudflareDohResolver {
    client: reqwest::Client,
    endpoint: String,
}
impl Default for CloudflareDohResolver {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: "https://cloudflare-dns.com/dns-query".into(),
        }
    }
}
#[derive(Deserialize)]
struct DnsReply {
    #[serde(rename = "Answer", default)]
    answer: Vec<DnsAnswer>,
}
#[derive(Deserialize)]
struct DnsAnswer {
    data: String,
}
#[async_trait]
impl DnsResolver for CloudflareDohResolver {
    async fn observes(&self, record: &RemoteRecord) -> Result<bool, DnsServiceError> {
        let response = self
            .client
            .get(&self.endpoint)
            .query(&[("name", record.name.as_str()), ("type", kind(record))])
            .header(reqwest::header::ACCEPT, "application/dns-json")
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|_| DnsServiceError::provider("resolver request"))?;
        if !response.status().is_success() {
            return Err(DnsServiceError::provider("resolver status"));
        }
        let reply: DnsReply = response
            .json()
            .await
            .map_err(|_| DnsServiceError::provider("resolver response"))?;
        let expected = normalize(&record.data.to_string());
        Ok(reply
            .answer
            .iter()
            .any(|answer| normalize(&answer.data) == expected))
    }
}
fn kind(record: &RemoteRecord) -> &'static str {
    use openpanel_domain::dns::RecordKind;
    match record.data.kind() {
        RecordKind::A => "A",
        RecordKind::Aaaa => "AAAA",
        RecordKind::Cname => "CNAME",
        RecordKind::Txt => "TXT",
        RecordKind::Mx => "MX",
        RecordKind::Caa => "CAA",
        RecordKind::Ns => "NS",
        RecordKind::Srv => "SRV",
    }
}
fn normalize(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}
