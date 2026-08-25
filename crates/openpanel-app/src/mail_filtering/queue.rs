//! Read-only Postfix outbound-queue adapter (`postqueue -j`).

use openpanel_domain::mail_filtering::{MailQueueSnapshot, MtaQueuePort};

/// Adapter shelling out to the managed MTA's queue tool. All failures
/// degrade to
/// [`QueueHealth::Unknown`](openpanel_domain::mail_filtering::QueueHealth::Unknown)
/// at the caller; nothing here
/// mutates queue state.
pub struct PostfixQueueAdapter {
    /// Queue binary (default `postqueue`).
    pub binary: String,
}

impl PostfixQueueAdapter {
    /// Construct the adapter for a specific binary path.
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
        }
    }
}

/// One parsed `postqueue -j` record. Only the fields needed for the
/// aggregate snapshot are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRecord {
    /// Named queue the message sits in (e.g. `active`, `deferred`).
    pub queue_name: String,
    /// Unix arrival time, when present.
    pub arrival_time: Option<i64>,
}

/// Parse `postqueue -j` output (one JSON object per line). Malformed
/// lines are skipped; a payload with zero parseable records yields an
/// empty snapshot rather than an error.
pub fn parse_queue_json(payload: &str) -> Vec<QueueRecord> {
    let mut records = Vec::new();
    for line in payload.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('{') {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        let queue_name = value
            .get("queue_name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let arrival_time = value
            .get("arrival_time")
            .and_then(serde_json::Value::as_i64);
        records.push(QueueRecord {
            queue_name,
            arrival_time,
        });
    }
    records
}

/// Aggregate parsed records into a snapshot.
pub fn snapshot_from_records(records: &[QueueRecord]) -> MailQueueSnapshot {
    let oldest_deferred = records
        .iter()
        .filter(|record| record.queue_name == "deferred")
        .filter_map(|record| record.arrival_time)
        .min()
        .map(|seconds| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)
                .unwrap_or_else(chrono::Utc::now)
        });
    MailQueueSnapshot::ok(records.len() as u64, oldest_deferred)
}

#[async_trait::async_trait]
impl MtaQueuePort for PostfixQueueAdapter {
    async fn snapshot(&self) -> Result<MailQueueSnapshot, String> {
        use tokio::io::AsyncReadExt;

        let mut child = tokio::process::Command::new(&self.binary)
            .arg("-j")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;

        let Some(mut stdout) = child.stdout.take() else {
            return Err("postqueue produced no stdout".into());
        };
        // Bounded read: 8 MiB is far beyond any realistic queue dump.
        const CAP: usize = 8 * 1024 * 1024;
        let mut payload = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let read = stdout.read(&mut chunk).await.map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            if payload.len() + read > CAP {
                return Err("queue dump exceeded size cap".into());
            }
            payload.extend_from_slice(&chunk[..read]);
        }
        let status = child.wait().await.map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("postqueue exited {status}"));
        }
        let text = String::from_utf8_lossy(&payload);
        Ok(snapshot_from_records(&parse_queue_json(&text)))
    }
}

/// Deterministic no-op adapter for fake/in-memory compositions.
pub struct NullQueueAdapter;

#[async_trait::async_trait]
impl MtaQueuePort for NullQueueAdapter {
    async fn snapshot(&self) -> Result<MailQueueSnapshot, String> {
        Ok(MailQueueSnapshot::ok(0, None))
    }
}

#[cfg(test)]
mod tests {
    use openpanel_domain::mail_filtering::QueueHealth;

    use super::*;

    #[test]
    fn test_parse_json_lines_and_aggregate() {
        let payload = concat!(
            r#"{"queue_name":"active","arrival_time":1700000000,"size":123}"#,
            "\n",
            r#"{"queue_name":"deferred","arrival_time":1699990000}"#,
            "\n",
            "not json at all\n",
            r#"{"queue_name":"deferred","arrival_time":1699980000}"#,
            "\n"
        );
        let records = parse_queue_json(payload);
        assert_eq!(records.len(), 3);
        let snapshot = snapshot_from_records(&records);
        assert_eq!(snapshot.queue_depth, 3);
        assert_eq!(snapshot.health, QueueHealth::Ok);
        let oldest = snapshot.oldest_deferred_at.expect("oldest deferred");
        assert_eq!(oldest.timestamp(), 1_699_980_000);
    }

    #[test]
    fn test_empty_payload_yields_ok_zero() {
        let snapshot = snapshot_from_records(&parse_queue_json(""));
        assert_eq!(snapshot.queue_depth, 0);
        assert_eq!(snapshot.health, QueueHealth::Ok);
        assert_eq!(snapshot.oldest_deferred_at, None);
    }

    #[tokio::test]
    async fn test_missing_binary_degrades_to_error_string() {
        let adapter = PostfixQueueAdapter::new("/nonexistent/postqueue");
        let result = adapter.snapshot().await;
        assert!(result.is_err(), "missing binary must surface an error");
    }
}
