# Add Log Viewer — Design

## LogSource model

```rust
pub enum LogSource {
    Site(SiteId, LogKind),   // access | error
    Audit,
    System(ServiceName),
}

pub struct LogQuery {
    pub source: LogSource,
    pub filter: Option<String>,   // substring / regex over line
    pub since: Option<Offset>,    // tail N lines or since time
    pub download: bool,
}
```

## LogLine model

```rust
pub struct LogLine {
    pub ts: DateTime<Utc>,
    pub source: LogSource,
    pub raw: String,
}
```

## Aggregation / authorization flow

```
read(query, caller):
  authorize(caller, query.source):
    Site(s, _) -> caller owns s OR caller is ServerAdmin
    Audit      -> caller is ServerAdmin
    System(_)  -> caller is ServerAdmin
  lines = aggregator.query(query)          // over JSONL export store
  if query.download: rate-limit + audit LogDownloaded{source}
  return lines (or stream for tail)
```

The aggregator reads the same JSONL files that `observability-export`
writes, so no new log pipeline is introduced.

## Endpoints

```
GET /api/v1/logs/sites/{id}?kind=access|error&filter=&tail=
GET /api/v1/logs/audit?filter=&tail=
GET /api/v1/logs/system/{service}?filter=&tail=
```

## Tests

```
1.1 Unit: RBAC authorize (owner site, foreign site, audit, system);
    query parsing (tail + filter).
1.2 Property: a returned line always belongs to the authorized source;
    tail(N) returns the last N lines in order.
1.3 Service tests w/ mock JSONL store: site, audit, system queries;
    download audit event.
1.4 Integration: owner sees own site logs, is denied another owner's
    site; Server Admin sees audit; filter narrows results.
1.5 CLI E2E: openpanel logs site <id> --tail 50.
1.6 Web: Logs page with kind tabs, search box, tail toggle, download.
```
