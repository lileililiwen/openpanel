//! In-memory `BinlogSink` and `LogTailer` implementations for
//! tests and offline development.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{BinlogRange, BinlogSegment, BinlogSink, LogSeq, LogTailer, PitrError};
use tokio::sync::Mutex;
use uuid::Uuid;

/// In-memory `BinlogSink` implementation. Segments live in a
/// `HashMap` keyed by `(database_id, from-hex)`. The available range
/// is derived from the inserted segments.
#[derive(Default, Clone)]
pub struct InMemoryBinlogSink {
    inner: Arc<Mutex<InMemoryBinlogSinkState>>,
}

#[derive(Default)]
struct InMemoryBinlogSinkState {
    segments: HashMap<(Uuid, String), BinlogSegment>,
    ranges: HashMap<Uuid, (LogSeq, LogSeq, DateTime<Utc>, DateTime<Utc>)>,
}

impl InMemoryBinlogSink {
    /// Build a new in-memory sink.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl BinlogSink for InMemoryBinlogSink {
    async fn append(
        &self,
        database_id: Uuid,
        segment: &BinlogSegment,
    ) -> Result<LogSeq, PitrError> {
        let mut g = self.inner.lock().await;
        g.segments
            .insert((database_id, segment.from.to_hex()), segment.clone());
        let entry = g.ranges.entry(database_id).or_insert_with(|| {
            (
                segment.from.clone(),
                segment.to.clone(),
                segment.generated_at,
                segment.generated_at,
            )
        });
        entry.1 = segment.to.clone();
        if segment.generated_at < entry.2 {
            entry.2 = segment.generated_at;
        }
        if segment.generated_at > entry.3 {
            entry.3 = segment.generated_at;
        }
        Ok(segment.to.clone())
    }

    async fn replay_window(
        &self,
        database_id: Uuid,
        _earliest: LogSeq,
        _target_ts: DateTime<Utc>,
    ) -> Result<Vec<BinlogSegment>, PitrError> {
        let g = self.inner.lock().await;
        let mut out: Vec<BinlogSegment> = g
            .segments
            .iter()
            .filter(|((db, _), _)| *db == database_id)
            .map(|(_, seg)| seg.clone())
            .collect();
        out.sort_by(|a, b| a.from.as_bytes().cmp(b.from.as_bytes()));
        Ok(out)
    }

    async fn available_range(&self, database_id: Uuid) -> Result<BinlogRange, PitrError> {
        let g = self.inner.lock().await;
        if let Some((earliest, latest, start, end)) = g.ranges.get(&database_id).cloned() {
            BinlogRange::new(earliest, latest, start, end)
        } else {
            let now = Utc::now();
            BinlogRange::new(LogSeq::initial(), LogSeq::initial(), now, now)
        }
    }
}

/// In-memory `LogTailer` that returns a pre-loaded queue of
/// segments per database. The `Default` impl produces an empty
/// tailer; tests use [`InMemoryLogTailer::new`] and
/// [`InMemoryLogTailer::enqueue`] to seed the queue.
#[derive(Default, Clone)]
pub struct InMemoryLogTailer {
    inner: Arc<Mutex<HashMap<Uuid, Vec<BinlogSegment>>>>,
}

impl InMemoryLogTailer {
    /// Build a new in-memory tailer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a segment that will be returned on the next call to
    /// [`LogTailer::next_segment`].
    pub async fn enqueue(&self, database_id: Uuid, segment: BinlogSegment) {
        let mut g = self.inner.lock().await;
        g.entry(database_id).or_default().push(segment);
    }
}

#[async_trait]
impl LogTailer for InMemoryLogTailer {
    async fn next_segment(
        &self,
        database_id: Uuid,
        _from: LogSeq,
    ) -> Result<Option<BinlogSegment>, PitrError> {
        let mut g = self.inner.lock().await;
        let q = g.entry(database_id).or_default();
        if q.is_empty() {
            Ok(None)
        } else {
            Ok(Some(q.remove(0)))
        }
    }

    async fn enable_streaming(&self, _database_id: Uuid) -> Result<(), PitrError> {
        Ok(())
    }

    async fn disable_streaming(&self, _database_id: Uuid) -> Result<(), PitrError> {
        Ok(())
    }
}
