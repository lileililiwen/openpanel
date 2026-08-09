## Context

OpenPanel already generates nginx vhosts and append-only audit rows. Log files are untrusted, potentially huge, actively rotated, and can contain credentials in URLs or headers.

## Goals / Non-Goals

**Goals:** recent site access/error inspection, audit browsing, bounded traffic summaries, rotation awareness, RBAC, and redaction.

**Non-Goals:** full-text indexing, SIEM replacement, arbitrary `/var/log` browsing, request-body capture, or AWStats parity.

## Decisions

1. Define a fixed OpenPanel nginx log format with site ID, timestamp, method, normalized path, status, bytes, duration, remote address, and user agent; never log query strings, cookies, authorization, or bodies.
2. Resolve only registered log sources under configured roots. Reads use cursor `(file identity, offset)`, byte/line caps, reverse-tail scanning, and rotation detection; user paths are never accepted.
3. Parse lines into typed records and degrade malformed lines to redacted raw entries. Aggregate hourly counts, bytes, status classes, and latency in SQLite with bounded retention.
4. Expose `/api/v1/logs`, `openpanel logs`, and `/logs`; follow uses HTMX polling/API cursors rather than WebSockets initially.

## Risks / Trade-offs

- Logs may contain personal data -> minimize fields, mask IPs for non-Owners, document retention, and audit downloads.
- High-volume parsing can consume resources -> incremental offsets, batch caps, backpressure, and configurable retention.
- Rotation races can duplicate entries -> stable cursor identity and at-least-once aggregation with unique source keys.

## Migration Plan

Render the new log format into managed vhosts and reload nginx transactionally. Historical incompatible lines remain viewable as redacted raw text but are not aggregated.
