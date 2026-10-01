//! Bounded newline-delimited transport using the existing provider codec and facade.
use super::{
    ForgeProviderProcessHandle, ForgePublishProvider, JsonRpcParseError, decode_request,
    encode_error, encode_response,
};
use async_trait::async_trait;
use openpanel_core::{
    CoreError, CoreResult,
    audit::{AuditEvent, AuditPage, AuditQuery, AuditService},
};
use openpanel_domain::{User, forge_publish_provider::ForgeProviderError};
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum frame bytes, excluding its newline. Excess bytes are drained without allocation.
pub const MAX_LINE_LEN: usize = 1024 * 1024;

/// Transport failures are fatal; malformed frames instead produce recoverable protocol errors.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// Reader or writer failed. Raw host diagnostics are never printed.
    #[error("provider transport I/O failed")]
    Io {
        /// Failure class, retained without request or host details.
        kind: std::io::ErrorKind,
    },
    /// Codec could not encode a response.
    #[error("provider response encoding failed")]
    Encode,
}

impl From<std::io::Error> for TransportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io { kind: error.kind() }
    }
}

/// Observes audit failures swallowed by the existing facade, without duplicating events.
/// Use this same instance for the facade, delivery services and transport.
pub struct ForgeProviderAudit {
    inner: Arc<dyn AuditService>,
    failed: AtomicBool,
}
impl ForgeProviderAudit {
    /// Wrap the provider's audit sink. Transport exchanges remain serialized.
    pub fn new(inner: Arc<dyn AuditService>) -> Self {
        Self {
            inner,
            failed: AtomicBool::new(false),
        }
    }
}
#[async_trait]
impl AuditService for ForgeProviderAudit {
    async fn record(&self, event: AuditEvent) -> CoreResult<()> {
        match self.inner.record(event).await {
            Ok(()) => Ok(()),
            Err(_) => {
                self.failed.store(true, Ordering::SeqCst);
                Err(CoreError::Io(std::io::Error::other(
                    "provider audit write failed",
                )))
            }
        }
    }
    async fn recent(&self, limit: i64) -> CoreResult<Vec<AuditEvent>> {
        self.inner.recent(limit).await
    }
    async fn query(&self, query: AuditQuery) -> CoreResult<AuditPage> {
        self.inner.query(query).await
    }
}

/// Serve requests until EOF. Notifications execute but produce no response, even on errors.
/// Handler panics and audit failures emit redacted -32603 errors and mark the lifecycle failed.
/// I/O failures end the loop; clean EOF stops its lifecycle. Request bodies are never logged.
pub async fn run_loop<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    mut reader: R,
    mut writer: W,
    provider: Arc<ForgePublishProvider>,
    caller: Arc<User>,
    lifecycle: &mut ForgeProviderProcessHandle,
    audit: Arc<ForgeProviderAudit>,
) -> Result<(), TransportError> {
    let result = serve(&mut reader, &mut writer, provider, caller, lifecycle, audit).await;
    match &result {
        Ok(()) => {
            lifecycle.stop_provider();
        }
        Err(_) => {
            lifecycle.mark_provider_failed();
        }
    }
    result
}
async fn serve<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
    provider: Arc<ForgePublishProvider>,
    caller: Arc<User>,
    lifecycle: &mut ForgeProviderProcessHandle,
    audit: Arc<ForgeProviderAudit>,
) -> Result<(), TransportError> {
    while let Some((bytes, oversized)) = read_frame(reader).await? {
        if oversized {
            let err = JsonRpcParseError::Request(ForgeProviderError::InvalidRevision(
                "frame too large".into(),
            ));
            write_frame(
                writer,
                &encode_error(&err, None).map_err(|_| TransportError::Encode)?,
            )
            .await?;
            continue;
        }
        let line = match std::str::from_utf8(&bytes) {
            Ok(line) => line.trim_end_matches('\r'),
            Err(_) => {
                let frame = encode_error(&JsonRpcParseError::Frame("invalid UTF-8".into()), None)
                    .map_err(|_| TransportError::Encode)?;
                write_frame(writer, &frame).await?;
                continue;
            }
        };
        let raw = serde_json::from_str::<Value>(line).ok();
        let notification = raw
            .as_ref()
            .is_some_and(|v| v.is_object() && v["jsonrpc"] == "2.0" && v.get("id").is_none());
        let id = raw.as_ref().and_then(|v| v.get("id")).cloned();
        let request = match decode_request(line) {
            Ok((request, _)) => request,
            Err(error) => {
                if !notification {
                    let safe = match error {
                        JsonRpcParseError::Frame(_) => {
                            JsonRpcParseError::Frame("invalid frame".into())
                        }
                        JsonRpcParseError::Method(_) => {
                            JsonRpcParseError::Method("unsupported method".into())
                        }
                        JsonRpcParseError::Request(_) => JsonRpcParseError::Request(
                            ForgeProviderError::InvalidRevision("invalid request".into()),
                        ),
                    };
                    write_frame(
                        writer,
                        &encode_error(&safe, id).map_err(|_| TransportError::Encode)?,
                    )
                    .await?;
                }
                continue;
            }
        };
        audit.failed.store(false, Ordering::SeqCst);
        let provider = provider.clone();
        let caller = caller.clone();
        // JoinError captures unwinding across await. Production provider installs a redacted panic hook.
        let response =
            tokio::spawn(async move { provider.handle_request(request, &caller).await }).await;
        let frame = match response {
            Ok(response) if !audit.failed.load(Ordering::SeqCst) => {
                encode_response(&response, id).map_err(|_| TransportError::Encode)?
            }
            _ => {
                lifecycle.mark_provider_failed();
                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32603,"message":"provider handler or audit failed"}}).to_string()
            }
        };
        if !notification {
            write_frame(writer, &frame).await?;
        }
    }
    Ok(())
}
async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &str,
) -> Result<(), TransportError> {
    writer.write_all(frame.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<(Vec<u8>, bool)>, TransportError> {
    let mut bytes = Vec::new();
    let mut oversized = false;
    let mut seen = false;
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(seen.then_some((bytes, oversized)));
        }
        seen = true;
        let newline = chunk.iter().position(|b| *b == b'\n');
        let count = newline.unwrap_or(chunk.len());
        if !oversized {
            if count > MAX_LINE_LEN.saturating_sub(bytes.len()) {
                oversized = true;
                bytes.clear();
            } else {
                bytes.extend_from_slice(&chunk[..count]);
            }
        }
        reader.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(Some((bytes, oversized)));
        }
    }
}
