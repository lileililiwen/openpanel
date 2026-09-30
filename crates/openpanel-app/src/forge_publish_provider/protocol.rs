//! JSON-RPC-over-stdio adapter for the optional Forge publish
//! provider. The adapter is transport-only: it decodes a
//! newline-delimited JSON-RPC 2.0 frame into a validated
//! `ForgeProviderRequest`, and encodes a `ForgeProviderResponse`
//! back into a JSON-RPC result frame. It never mutates state and
//! never sees a secret value (the wire contract forbids one).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use openpanel_domain::forge_publish_provider::{
    FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION, ForgeProviderError, ForgeProviderRequest,
    ForgeProviderResponse, parse_request,
};

/// JSON-RPC method the Forge publish provider answers. The Forge
/// operation is carried in `params.operation`, not the method.
pub const FORGE_JSONRPC_METHOD: &str = "forge.publish";

/// JSON-RPC 2.0 version string.
pub const JSONRPC_VERSION: &str = "2.0";

/// Inbound JSON-RPC request envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForgeProviderRequestEnvelope {
    /// MUST be `"2.0"`.
    pub jsonrpc: String,
    /// Opaque correlation id; echoed on the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// MUST be [`FORGE_JSONRPC_METHOD`].
    pub method: String,
    /// The wire-level `forge-publish-provider/0.1.0` request.
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeProviderErrorObject {
    /// Numeric JSON-RPC error code.
    pub code: i64,
    /// Caller-safe message (never a secret value).
    pub message: String,
}

/// Outbound JSON-RPC response envelope. Exactly one of `result`
/// or `error` is set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForgeProviderResponseEnvelope {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Echoed correlation id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// Successful result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ForgeProviderResponse>,
    /// Error result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ForgeProviderErrorObject>,
}

/// Typed parse failures raised by the JSON-RPC adapter. Every
/// failure is reported to Forge as a JSON-RPC error frame; none of
/// the variants carries a secret value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JsonRpcParseError {
    /// The frame is not a well-formed JSON-RPC envelope.
    #[error("invalid JSON-RPC frame: {0}")]
    Frame(String),
    /// The frame declared an unsupported JSON-RPC method.
    #[error("unsupported JSON-RPC method: {0}")]
    Method(String),
    /// The params carried an invalid Forge publish request.
    #[error("invalid Forge publish request: {0}")]
    Request(ForgeProviderError),
}

impl JsonRpcParseError {
    /// JSON-RPC error code for this failure.
    pub fn json_rpc_code(&self) -> i64 {
        match self {
            Self::Frame(_) => -32700,
            Self::Method(_) => -32601,
            Self::Request(_) => -32602,
        }
    }

    /// Caller-safe message.
    pub fn message(&self) -> String {
        self.to_string()
    }
}

/// Stateless JSON-RPC codec. Kept as a unit struct so the adapter
/// composition can name it without owning transport state.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonRpcCodec;

impl JsonRpcCodec {
    /// Decode one newline-delimited JSON-RPC frame into a validated
    /// Forge request plus its correlation id.
    pub fn decode_line(
        line: &str,
    ) -> Result<(ForgeProviderRequest, Option<serde_json::Value>), JsonRpcParseError> {
        decode_request(line)
    }

    /// Encode a terminal Forge response as a JSON-RPC result frame.
    pub fn encode_line(
        response: &ForgeProviderResponse,
        id: Option<serde_json::Value>,
    ) -> Result<String, JsonRpcParseError> {
        encode_response(response, id)
    }
}

/// Decode a JSON-RPC frame string into a validated Forge publish
/// request.
pub fn decode_request(
    frame: &str,
) -> Result<(ForgeProviderRequest, Option<serde_json::Value>), JsonRpcParseError> {
    let envelope: ForgeProviderRequestEnvelope =
        serde_json::from_str(frame).map_err(|err| JsonRpcParseError::Frame(err.to_string()))?;
    if envelope.jsonrpc != JSONRPC_VERSION {
        return Err(JsonRpcParseError::Frame(format!(
            "unsupported jsonrpc version: {}",
            envelope.jsonrpc
        )));
    }
    if envelope.method != FORGE_JSONRPC_METHOD {
        return Err(JsonRpcParseError::Method(envelope.method));
    }
    let request = decode_envelope(&envelope.params)?;
    Ok((request, envelope.id))
}

/// Decode the `params` value of a frame into a validated Forge
/// publish request.
pub fn decode_envelope(
    params: &serde_json::Value,
) -> Result<ForgeProviderRequest, JsonRpcParseError> {
    let request = parse_request(params).map_err(JsonRpcParseError::Request)?;
    if request.protocol != FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION {
        return Err(JsonRpcParseError::Request(
            ForgeProviderError::UnsupportedProtocol(request.protocol),
        ));
    }
    Ok(request)
}

/// Encode a terminal Forge response as a newline-delimited
/// JSON-RPC result frame.
pub fn encode_response(
    response: &ForgeProviderResponse,
    id: Option<serde_json::Value>,
) -> Result<String, JsonRpcParseError> {
    let envelope = ForgeProviderResponseEnvelope {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        result: Some(response.clone()),
        error: None,
    };
    serde_json::to_string(&envelope).map_err(|err| JsonRpcParseError::Frame(err.to_string()))
}

/// Encode a parse failure as a newline-delimited JSON-RPC error
/// frame.
pub fn encode_error(
    error: &JsonRpcParseError,
    id: Option<serde_json::Value>,
) -> Result<String, JsonRpcParseError> {
    let envelope = ForgeProviderResponseEnvelope {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        result: None,
        error: Some(ForgeProviderErrorObject {
            code: error.json_rpc_code(),
            message: error.message(),
        }),
    };
    serde_json::to_string(&envelope).map_err(|err| JsonRpcParseError::Frame(err.to_string()))
}
