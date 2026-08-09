//! Cloudflare DNS v4 API adapter.

use async_trait::async_trait;
use openpanel_domain::dns::{DnsName, ProviderCapabilities, RecordData, RecordKind, RemoteVersion};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{DnsProvider, DnsServiceError, ProviderCredential, ProviderZone, RemoteRecord};

/// Cloudflare API-token DNS adapter.
pub struct CloudflareProvider {
    client: reqwest::Client,
    base: String,
}
impl Default for CloudflareProvider {
    fn default() -> Self {
        Self::new("https://api.cloudflare.com/client/v4")
    }
}
impl CloudflareProvider {
    /// Construct with an overridable base URL for contract tests.
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base: base.into().trim_end_matches('/').to_owned(),
        }
    }

    async fn get(
        &self,
        path: &str,
        credential: &ProviderCredential,
    ) -> Result<Value, DnsServiceError> {
        let response = self
            .client
            .get(format!("{}{}", self.base, path))
            .bearer_auth(credential.expose())
            .send()
            .await
            .map_err(|_| DnsServiceError::provider("request"))?;
        decode(response).await
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        credential: &ProviderCredential,
        body: Option<Value>,
    ) -> Result<Value, DnsServiceError> {
        let mut request = self
            .client
            .request(method, format!("{}{}", self.base, path))
            .bearer_auth(credential.expose());
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|_| DnsServiceError::provider("request"))?;
        decode(response).await
    }

    async fn current_record(
        &self,
        credential: &ProviderCredential,
        zone_id: &str,
        record_id: &str,
    ) -> Result<RemoteRecord, DnsServiceError> {
        let value = self
            .get(
                &format!("/zones/{zone_id}/dns_records/{record_id}"),
                credential,
            )
            .await?;
        parse_record(&value)
    }
}
#[derive(Deserialize)]
struct Envelope {
    success: bool,
    result: Value,
}
async fn decode(response: reqwest::Response) -> Result<Value, DnsServiceError> {
    if !response.status().is_success() {
        return Err(DnsServiceError::provider("provider request failed"));
    }
    let envelope: Envelope = response
        .json()
        .await
        .map_err(|_| DnsServiceError::provider("invalid response"))?;
    if !envelope.success {
        return Err(DnsServiceError::provider("provider request failed"));
    }
    Ok(envelope.result)
}
fn version(value: &Value) -> Result<RemoteVersion, DnsServiceError> {
    RemoteVersion::new(
        value
            .get("modified_on")
            .and_then(Value::as_str)
            .or_else(|| value.get("created_on").and_then(Value::as_str))
            .or_else(|| value.get("id").and_then(Value::as_str))
            .ok_or(DnsServiceError::Provider(
                "provider response invalid".into(),
            ))?,
    )
    .map_err(|_| DnsServiceError::Provider("provider response invalid".into()))
}
fn parse_record(value: &Value) -> Result<RemoteRecord, DnsServiceError> {
    let kind = parse_kind(value.get("type").and_then(Value::as_str).ok_or(
        DnsServiceError::Provider("provider response invalid".into()),
    )?)?;
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let presentation = match kind {
        RecordKind::Mx => format!(
            "{} {}",
            value.get("priority").and_then(Value::as_u64).unwrap_or(0),
            content
        ),
        RecordKind::Srv => {
            let data = value.get("data").ok_or(DnsServiceError::Provider(
                "provider response invalid".into(),
            ))?;
            format!(
                "{} {} {} {}",
                data.get("priority").and_then(Value::as_u64).unwrap_or(0),
                data.get("weight").and_then(Value::as_u64).unwrap_or(0),
                data.get("port").and_then(Value::as_u64).unwrap_or(0),
                data.get("target")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            )
        }
        RecordKind::Caa => {
            let data = value.get("data").ok_or(DnsServiceError::Provider(
                "provider response invalid".into(),
            ))?;
            format!(
                "{} {} {}",
                data.get("flags").and_then(Value::as_u64).unwrap_or(0),
                data.get("tag").and_then(Value::as_str).unwrap_or_default(),
                data.get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            )
        }
        _ => content.to_owned(),
    };
    Ok(RemoteRecord {
        remote_id: value
            .get("id")
            .and_then(Value::as_str)
            .ok_or(DnsServiceError::Provider(
                "provider response invalid".into(),
            ))?
            .to_owned(),
        name: DnsName::new(value.get("name").and_then(Value::as_str).ok_or(
            DnsServiceError::Provider("provider response invalid".into()),
        )?)
        .map_err(|_| DnsServiceError::Provider("provider response invalid".into()))?,
        data: RecordData::parse(kind, &presentation)
            .map_err(|_| DnsServiceError::Provider("provider response invalid".into()))?,
        ttl: u32::try_from(value.get("ttl").and_then(Value::as_u64).unwrap_or(1))
            .map_err(|_| DnsServiceError::Provider("provider response invalid".into()))?,
        remote_version: version(value)?,
    })
}
fn parse_kind(value: &str) -> Result<RecordKind, DnsServiceError> {
    match value {
        "A" => Ok(RecordKind::A),
        "AAAA" => Ok(RecordKind::Aaaa),
        "CNAME" => Ok(RecordKind::Cname),
        "TXT" => Ok(RecordKind::Txt),
        "MX" => Ok(RecordKind::Mx),
        "CAA" => Ok(RecordKind::Caa),
        "NS" => Ok(RecordKind::Ns),
        "SRV" => Ok(RecordKind::Srv),
        _ => Err(DnsServiceError::Provider(
            "unsupported provider record".into(),
        )),
    }
}
fn payload(record: &RemoteRecord) -> Value {
    let mut value =
        json!({"type":kind_name(record.data.kind()),"name":record.name.as_str(),"ttl":record.ttl});
    match &record.data {
        RecordData::Mx { priority, exchange } => {
            value["priority"] = json!(priority);
            value["content"] = json!(exchange.as_str());
        }
        RecordData::Srv {
            priority,
            weight,
            port,
            target,
        } => {
            value["data"] =
                json!({"priority":priority,"weight":weight,"port":port,"target":target.as_str()})
        }
        RecordData::Caa {
            flags,
            tag,
            value: authority,
        } => value["data"] = json!({"flags":flags,"tag":tag,"value":authority}),
        data => value["content"] = json!(data.to_string()),
    };
    value
}
fn kind_name(kind: RecordKind) -> &'static str {
    match kind {
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
fn stable_id(remote_id: &str) -> Uuid {
    let digest = Sha256::digest(remote_id.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}
#[async_trait]
impl DnsProvider for CloudflareProvider {
    async fn test(
        &self,
        credential: &ProviderCredential,
    ) -> Result<ProviderCapabilities, DnsServiceError> {
        let _ = self.get("/zones?per_page=1", credential).await?;
        Ok(ProviderCapabilities::all(60, 86_400))
    }

    async fn zones(
        &self,
        credential: &ProviderCredential,
    ) -> Result<Vec<ProviderZone>, DnsServiceError> {
        let result = self.get("/zones?per_page=50", credential).await?;
        let zones = result.as_array().ok_or(DnsServiceError::Provider(
            "provider response invalid".into(),
        ))?;
        zones
            .iter()
            .map(|zone| {
                let remote_id =
                    zone.get("id")
                        .and_then(Value::as_str)
                        .ok_or(DnsServiceError::Provider(
                            "provider response invalid".into(),
                        ))?;
                Ok(ProviderZone::new(
                    stable_id(remote_id),
                    remote_id,
                    DnsName::new(zone.get("name").and_then(Value::as_str).ok_or(
                        DnsServiceError::Provider("provider response invalid".into()),
                    )?)
                    .map_err(|_| DnsServiceError::Provider("provider response invalid".into()))?,
                    version(zone)?,
                ))
            })
            .collect()
    }

    async fn records(
        &self,
        credential: &ProviderCredential,
        zone_id: &str,
    ) -> Result<Vec<RemoteRecord>, DnsServiceError> {
        let result = self
            .get(
                &format!("/zones/{zone_id}/dns_records?per_page=5000"),
                credential,
            )
            .await?;
        result
            .as_array()
            .ok_or(DnsServiceError::Provider(
                "provider response invalid".into(),
            ))?
            .iter()
            .map(parse_record)
            .collect()
    }

    async fn create_record(
        &self,
        credential: &ProviderCredential,
        zone_id: &str,
        record: &RemoteRecord,
        _expected: &RemoteVersion,
    ) -> Result<RemoteRecord, DnsServiceError> {
        let result = self
            .send(
                reqwest::Method::POST,
                &format!("/zones/{zone_id}/dns_records"),
                credential,
                Some(payload(record)),
            )
            .await?;
        parse_record(&result)
    }

    async fn update_record(
        &self,
        credential: &ProviderCredential,
        zone_id: &str,
        record: &RemoteRecord,
        expected: &RemoteVersion,
    ) -> Result<RemoteRecord, DnsServiceError> {
        let current = self
            .current_record(credential, zone_id, &record.remote_id)
            .await?;
        if &current.remote_version != expected {
            return Err(DnsServiceError::Conflict);
        }
        let result = self
            .send(
                reqwest::Method::PUT,
                &format!("/zones/{zone_id}/dns_records/{}", record.remote_id),
                credential,
                Some(payload(record)),
            )
            .await?;
        parse_record(&result)
    }

    async fn delete_record(
        &self,
        credential: &ProviderCredential,
        zone_id: &str,
        record_id: &str,
        expected: &RemoteVersion,
    ) -> Result<RemoteVersion, DnsServiceError> {
        let current = self.current_record(credential, zone_id, record_id).await?;
        if &current.remote_version != expected {
            return Err(DnsServiceError::Conflict);
        }
        let _ = self
            .send(
                reqwest::Method::DELETE,
                &format!("/zones/{zone_id}/dns_records/{record_id}"),
                credential,
                None,
            )
            .await?;
        Ok(current.remote_version)
    }
}
