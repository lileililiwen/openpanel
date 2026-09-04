//! Metadata redaction: secrets never reach a rendered audit view.

use serde_json::Value;

/// Metadata keys that are never rendered, regardless of value.
const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "token",
    "secret",
    "key",
    "api_key",
    "apikey",
    "private",
    "pem",
    "cert",
    "certificate",
    "csr",
    "credential",
    "credentials",
    "body",
    "passphrase",
    "auth",
    "authorization",
    "session",
    "cookie",
    "bearer",
    "otp",
    "signature",
    "salt",
    "webhook",
    "hook",
    "ssh",
    "private_key",
];

/// Whether a metadata key names a secret and must be dropped.
fn is_secret_key(key: &str) -> bool {
    let k = key.to_lowercase();
    SECRET_KEYS.iter().any(|s| k.contains(s))
}

/// Whether a string value looks like a secret (PEM, JWT, or a long
/// high-entropy blob) and should be replaced.
fn looks_like_secret(value: &str) -> bool {
    let v = value.trim();
    if v.starts_with("-----BEGIN") {
        return true;
    }
    if v.starts_with("eyJ") && v.split('.').count() == 3 {
        return true;
    }
    let all_alnum = v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'));
    if v.len() >= 40 && all_alnum {
        return true;
    }
    let all_hex = !v.is_empty() && v.chars().all(|c| c.is_ascii_hexdigit());
    if v.len() >= 64 && all_hex {
        return true;
    }
    false
}

/// Recursively redact secret-shaped keys and values from an audit
/// metadata document. Secret keys are dropped; secret-looking string
/// values are replaced with a redaction marker; everything else is kept.
pub fn redact_metadata(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if is_secret_key(k) {
                    continue;
                }
                out.insert(k.clone(), redact_metadata(v));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(redact_metadata).collect()),
        Value::String(s) => {
            if looks_like_secret(s) {
                Value::String("***redacted***".to_string())
            } else {
                Value::String(s.clone())
            }
        }
        other => other.clone(),
    }
}
