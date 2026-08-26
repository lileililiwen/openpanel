# Refine Mail with end-user surfaces — Design

## Explore & Reuse

Everything in this change already exists below the adapter layer; the
work is surfacing + two small app fixes:

- `crates/openpanel-domain/src/mail_filtering/mod.rs` — `AutoResponder`
  (:157–186), `Forwarder`/`ForwardLoop` (:189,239–254).
- `crates/openpanel-app/src/mail_filtering/{service.rs,repo.rs,module.rs}`
  — services, SQLite tables (`sieve_scripts`, `mailing_lists`),
  registered module.
- Alias CRUD precedent incl. REST:
  `crates/openpanel-app/src/mail/mod.rs:150–159`,
  `crates/openpanel-api/src/routes/mail.rs:185–209`.
- Queue stub to replace: `crates/openpanel-app/src/mail/mod.rs:621`
  (`queue_depth: 0`) and `MailStatus.health` always `"ready"`
  (:616–624).
- Web tab precedent: webmail-client UI under `crates/openpanel-web/`.
- Live spec gap: archived delta
  `archive/2026-08-15-...-add-mail-anti-spam-and-filtering/specs/mail/spec.md`
  holds requirements (Sieve/Autoresponder/Forwarders/Catch-all/Mailing
  Lists) that were never merged into `openspec/specs/mail/spec.md`.
  This change's tasks include folding them in via `openspec archive`
  merge so the new surface specs sit on a complete source of truth.

## New domain types (small)

```rust
pub struct MailQueueSnapshot {   // read model, no secrets
    pub domain_id: DomainId,
    pub queue_depth: u64,
    pub oldest_deferred_at: Option<DateTime<Utc>>,
}
pub trait MtaQueuePort: Send + Sync {
    async fn snapshot(&self, domain: &DomainId) -> Result<MailQueueSnapshot, MailError>;
}
```

App impl shells out read-only to the managed MTA queue tool
(`postqueue -j` style JSON) with output size cap; failures degrade to
`health = "unknown"`, never panic.

## Endpoints (mirror archived delta paths)

```
GET/PUT /api/v1/mail/mailboxes/{id}/filters         (Sieve)
PUT     /api/v1/mail/mailboxes/{id}/autoresponder
DELETE  /api/v1/mail/mailboxes/{id}/autoresponder
GET/PUT /api/v1/mail/domains/{id}/forwarders
GET/PUT /api/v1/mail/domains/{id}/catchall
GET/POST/DELETE /api/v1/mail/lists[/{list}/members]
GET     /api/v1/mail/domains/{id}/queue             (new)
```

CLI: `openpanel mail {filter,autoresponder,forwarder,catchall,list,queue}`.
Web: Mail tabs — Sieve editor (textarea + size counter), autoresponder
form with datetime pickers, forwarder table, list member editor.

## RBAC

Owner manages own domains/mailboxes; collaborators per existing scoped
access; Admin override. Reuse `RequireRole`/`AuthSessionExt`.

## Layering

Domain gains only `MailQueueSnapshot` + `MtaQueuePort`. App wires the
adapter. API/CLI/web are pure adapters over existing services.
