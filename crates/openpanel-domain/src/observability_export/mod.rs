//! Observability-export domain — Prometheus exposition, OTLP
//! trace/log intake, and a typed `ObservabilityConfig`.
//!
//! All types are pure data (no I/O). The application layer is
//! responsible for collecting metrics, accepting OTLP HTTP
//! bodies, and serving `/metrics`.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A typed error raised by the observability-export domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityError(pub String);

impl fmt::Display for ObservabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ObservabilityError {}

/// A single Prometheus metric sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum MetricSample {
    /// A monotonically increasing counter.
    Counter {
        name: String,
        labels: BTreeMap<String, String>,
        value: u64,
    },
    /// An instantaneous value (e.g. quota usage).
    Gauge {
        name: String,
        labels: BTreeMap<String, String>,
        value: f64,
    },
    /// A bucketed observation.
    Histogram {
        name: String,
        labels: BTreeMap<String, String>,
        buckets: Vec<HistogramBucket>,
        sum: f64,
        count: u64,
    },
}

/// One bucket of a histogram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistogramBucket {
    /// The upper bound of this bucket (inclusive). `f64::INFINITY`
    /// represents the +Inf bucket per Prometheus convention.
    pub le: f64,
    /// The cumulative count of observations <= `le`.
    pub count: u64,
}

impl MetricSample {
    /// The metric name.
    pub fn name(&self) -> &str {
        match self {
            MetricSample::Counter { name, .. }
            | MetricSample::Gauge { name, .. }
            | MetricSample::Histogram { name, .. } => name,
        }
    }

    /// The labels attached to the sample.
    pub fn labels(&self) -> &BTreeMap<String, String> {
        match self {
            MetricSample::Counter { labels, .. }
            | MetricSample::Gauge { labels, .. }
            | MetricSample::Histogram { labels, .. } => labels,
        }
    }
}

/// Configuration for the observability surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Whether the Prometheus `/metrics` endpoint is enabled.
    pub prometheus_enabled: bool,
    /// Whether the OTLP HTTP receiver is enabled.
    pub otlp_enabled: bool,
    /// Whether the JSONL log export endpoint is enabled.
    pub logs_export_enabled: bool,
    /// Optional bearer token required to push traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otlp_bearer: Option<String>,
    /// Histogram buckets for the HTTP latency histogram.
    /// The default matches the Prometheus recommended buckets.
    #[serde(default = "default_buckets")]
    pub http_latency_buckets: Vec<f64>,
}

fn default_buckets() -> Vec<f64> {
    vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            prometheus_enabled: true,
            otlp_enabled: true,
            logs_export_enabled: true,
            otlp_bearer: None,
            http_latency_buckets: default_buckets(),
        }
    }
}

/// Render a list of samples in the Prometheus text exposition
/// format (version 0.0.4). The output is a single string with
/// one sample per line. Label values are escaped per the
/// Prometheus convention (`\` → `\\`, `"` → `\"`, newline →
/// `\n`).
pub fn render_prometheus(samples: &[MetricSample]) -> String {
    let mut out = String::new();
    for sample in samples {
        match sample {
            MetricSample::Counter { name, labels, value } => {
                push_help_and_type(&mut out, name, "counter");
                out.push_str(name);
                if !labels.is_empty() {
                    out.push('{');
                    let mut first = true;
                    for (k, v) in labels {
                        if !first {
                            out.push(',');
                        }
                        first = false;
                        out.push_str(k);
                        out.push_str("=\"");
                        push_escaped(&mut out, v);
                        out.push('"');
                    }
                    out.push('}');
                }
                out.push(' ');
                out.push_str(&value.to_string());
                out.push('\n');
            }
            MetricSample::Gauge { name, labels, value } => {
                push_help_and_type(&mut out, name, "gauge");
                out.push_str(name);
                if !labels.is_empty() {
                    out.push('{');
                    let mut first = true;
                    for (k, v) in labels {
                        if !first {
                            out.push(',');
                        }
                        first = false;
                        out.push_str(k);
                        out.push_str("=\"");
                        push_escaped(&mut out, v);
                        out.push('"');
                    }
                    out.push('}');
                }
                out.push(' ');
                out.push_str(&format_value(*value));
                out.push('\n');
            }
            MetricSample::Histogram { name, labels, buckets, sum, count } => {
                push_help_and_type(&mut out, name, "histogram");
                for bucket in buckets {
                    out.push_str(name);
                    out.push_str("_bucket{");
                    let mut first = true;
                    for (k, v) in labels {
                        if !first {
                            out.push(',');
                        }
                        first = false;
                        out.push_str(k);
                        out.push_str("=\"");
                        push_escaped(&mut out, v);
                        out.push('"');
                    }
                    if !labels.is_empty() {
                        out.push(',');
                    }
                    out.push_str("le=\"");
                    if bucket.le.is_finite() {
                        out.push_str(&format_value(bucket.le));
                    } else {
                        out.push_str("+Inf");
                    }
                    out.push_str("\"} ");
                    out.push_str(&bucket.count.to_string());
                    out.push('\n');
                }
                // _sum and _count
                out.push_str(name);
                out.push_str("_sum");
                if !labels.is_empty() {
                    out.push('{');
                    let mut first = true;
                    for (k, v) in labels {
                        if !first {
                            out.push(',');
                        }
                        first = false;
                        out.push_str(k);
                        out.push_str("=\"");
                        push_escaped(&mut out, v);
                        out.push('"');
                    }
                    out.push('}');
                }
                out.push(' ');
                out.push_str(&format_value(*sum));
                out.push('\n');
                out.push_str(name);
                out.push_str("_count");
                if !labels.is_empty() {
                    out.push('{');
                    let mut first = true;
                    for (k, v) in labels {
                        if !first {
                            out.push(',');
                        }
                        first = false;
                        out.push_str(k);
                        out.push_str("=\"");
                        push_escaped(&mut out, v);
                        out.push('"');
                    }
                    out.push('}');
                }
                out.push(' ');
                out.push_str(&count.to_string());
                out.push('\n');
            }
        }
    }
    out
}

fn push_help_and_type(out: &mut String, name: &str, kind: &str) {
    out.push_str("# HELP ");
    out.push_str(name);
    out.push(' ');
    out.push_str(name);
    out.push(' ');
    out.push_str(kind);
    out.push('\n');
    out.push_str("# TYPE ");
    out.push_str(name);
    out.push(' ');
    out.push_str(kind);
    out.push('\n');
}

fn push_escaped(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
}

fn format_value(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value > 0.0 { "+Inf".to_string() } else { "-Inf".to_string() }
    } else {
        // Prometheus accepts Go's strconv format; emit a stable
        // representation with no trailing zeros when possible.
        let s = format!("{value}");
        s
    }
}

/// Redact a string for safe inclusion in an OTLP log body. The
/// redaction replaces the value with `<redacted>` if it looks
/// like a bearer token, password, or PEM private key.
pub fn redact_otlp_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return value.to_string();
    }
    // Bearer token (header value)
    if trimmed.len() >= 32 && trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return "<redacted>".to_string();
    }
    // PEM private key
    if trimmed.starts_with("-----BEGIN") && trimmed.contains("PRIVATE KEY") {
        return "<redacted:private-key>".to_string();
    }
    // Password-shaped value
    if trimmed.len() >= 12
        && trimmed.chars().any(|c| c.is_ascii_alphabetic())
        && trimmed.chars().any(|c| c.is_ascii_digit())
    {
        return "<redacted>".to_string();
    }
    value.to_string()
}

/// One row of the JSONL log export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogExportRow {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
}

impl LogExportRow {
    /// Render the row as a single line of JSON with control
    /// characters escaped (so the result is valid JSONL).
    pub fn to_jsonl(&self) -> String {
        let mut out = String::new();
        out.push('{');
        push_kv(&mut out, "timestamp", &self.timestamp, true);
        out.push(',');
        push_kv(&mut out, "level", &self.level, false);
        if let Some(actor) = &self.actor {
            out.push(',');
            push_kv(&mut out, "actor", actor, false);
        }
        if let Some(action) = &self.action {
            out.push(',');
            push_kv(&mut out, "action", action, false);
        }
        if !self.fields.is_empty() {
            out.push(',');
            out.push_str("\"fields\":{");
            let mut first = true;
            for (k, v) in &self.fields {
                if !first {
                    out.push(',');
                }
                first = false;
                push_kv(&mut out, k, v, false);
            }
            out.push('}');
        }
        out.push('}');
        out
    }
}

fn push_kv(out: &mut String, key: &str, value: &str, first: bool) {
    if !first && out.ends_with(',') {
        // no-op: caller already appended comma
    }
    out.push('"');
    push_escaped(out, key);
    out.push_str("\":\"");
    push_escaped(out, value);
    out.push('"');
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn empty_labels() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    #[test]
    fn counter_renders_prometheus_line() {
        let sample = MetricSample::Counter {
            name: "openpanel_login_total".to_string(),
            labels: empty_labels(),
            value: 42,
        };
        let out = render_prometheus(&[sample]);
        assert!(out.contains("# TYPE openpanel_login_total counter"));
        assert!(out.contains("openpanel_login_total 42"));
    }

    #[test]
    fn counter_with_labels_renders_braces() {
        let mut labels = BTreeMap::new();
        labels.insert("result".to_string(), "success".to_string());
        let sample = MetricSample::Counter {
            name: "openpanel_login_total".to_string(),
            labels,
            value: 7,
        };
        let out = render_prometheus(&[sample]);
        assert!(out.contains("openpanel_login_total{result=\"success\"} 7"));
    }

    #[test]
    fn gauge_renders_with_decimal() {
        let sample = MetricSample::Gauge {
            name: "openpanel_quota_used".to_string(),
            labels: empty_labels(),
            value: 12.5,
        };
        let out = render_prometheus(&[sample]);
        assert!(out.contains("# TYPE openpanel_quota_used gauge"));
        assert!(out.contains("openpanel_quota_used 12.5"));
    }

    #[test]
    fn histogram_renders_buckets_and_sum_count() {
        let mut labels = BTreeMap::new();
        labels.insert("route".to_string(), "/api/v1/identity/login".to_string());
        let sample = MetricSample::Histogram {
            name: "openpanel_http_request_duration_seconds".to_string(),
            labels,
            buckets: vec![
                HistogramBucket { le: 0.01, count: 5 },
                HistogramBucket { le: 0.1, count: 9 },
                HistogramBucket { le: f64::INFINITY, count: 10 },
            ],
            sum: 0.42,
            count: 10,
        };
        let out = render_prometheus(&[sample]);
        assert!(out.contains("openpanel_http_request_duration_seconds_bucket{route=\"/api/v1/identity/login\",le=\"0.01\"} 5"));
        assert!(out.contains("openpanel_http_request_duration_seconds_bucket{route=\"/api/v1/identity/login\",le=\"0.1\"} 9"));
        assert!(out.contains("openpanel_http_request_duration_seconds_bucket{route=\"/api/v1/identity/login\",le=\"+Inf\"} 10"));
        assert!(out.contains("openpanel_http_request_duration_seconds_sum{route=\"/api/v1/identity/login\"} 0.42"));
        assert!(out.contains("openpanel_http_request_duration_seconds_count{route=\"/api/v1/identity/login\"} 10"));
    }

    #[test]
    fn label_escapes_backslash_and_quote() {
        let mut labels = BTreeMap::new();
        labels.insert("path".to_string(), "C:\\foo\"bar".to_string());
        let sample = MetricSample::Counter {
            name: "x_total".to_string(),
            labels,
            value: 1,
        };
        let out = render_prometheus(&[sample]);
        assert!(out.contains("path=\"C:\\\\foo\\\"bar\""));
    }

    #[test]
    fn redact_otlp_value_strips_bearer_token() {
        let token = "a".repeat(48);
        assert_eq!(redact_otlp_value(&token), "<redacted>");
    }

    #[test]
    fn redact_otlp_value_strips_pem_private_key() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nABC\n-----END RSA PRIVATE KEY-----";
        assert_eq!(redact_otlp_value(pem), "<redacted:private-key>");
    }

    #[test]
    fn redact_otlp_value_keeps_short_strings() {
        assert_eq!(redact_otlp_value("hello"), "hello");
        assert_eq!(redact_otlp_value("ab"), "ab");
    }

    #[test]
    fn redact_otlp_value_keeps_pem_certificate() {
        let pem = "-----BEGIN CERTIFICATE-----\nMIID...\n-----END CERTIFICATE-----";
        assert_eq!(redact_otlp_value(pem), pem);
    }

    #[test]
    fn log_export_row_to_jsonl_escapes_newline() {
        let mut fields = BTreeMap::new();
        fields.insert("k".to_string(), "line1\nline2".to_string());
        let row = LogExportRow {
            timestamp: "2026-08-13T00:00:00Z".to_string(),
            level: "info".to_string(),
            message: "ok".to_string(),
            actor: Some("alice".to_string()),
            action: Some("login".to_string()),
            fields,
        };
        let line = row.to_jsonl();
        // The embedded newline is escaped, so the output is a
        // single line.
        assert_eq!(line.matches('\n').count(), 0);
        assert!(line.contains("line1\\nline2"));
    }
}
