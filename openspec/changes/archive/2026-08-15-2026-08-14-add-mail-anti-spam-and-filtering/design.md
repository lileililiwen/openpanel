# Add Mail anti-spam and filtering — Design

## AntiSpamPolicy model

```rust
pub struct AntiSpamPolicy {
    pub domain_id: DomainId,
    pub engine: SpamEngine,        // SpamAssassin | Rspamd
    pub score_threshold: f32,      // route to spam folder above this
    pub greylist_enabled: bool,
    pub learn_enabled: bool,       // Bayesian autolearn
}
```

## Mailbox filter / forwarding models

```rust
pub struct SieveScript {
    pub mailbox_id: MailboxId,
    pub script: String,            // size-capped (e.g. 64 KiB)
    pub active: bool,
}

pub struct AutoResponder {
    pub mailbox_id: MailboxId,
    pub subject: String,
    pub body: String,              // plaintext only
    pub starts_at: Option<DateTime>,
    pub ends_at: Option<DateTime>,
}

pub struct Forwarder {
    pub domain_id: DomainId,
    pub source: String,            // local part or "*"
    pub destination: String,       // remote or local address
}

pub struct CatchAll {
    pub domain_id: DomainId,
    pub destination: String,
}

pub struct MailingList {
    pub domain_id: DomainId,
    pub name: String,
    pub moderation: bool,
    pub subscribers: Vec<String>,
}
```

## Scoring / routing flow

```
inbound_mail(domain, mailbox, message):
  score = SpamScorer.score(message)         // headers + bayes only
  if score >= policy.score_threshold:
      deliver to mailbox/Spam (no body in logs)
  if policy.greylist_enabled and first_seen:
      defer (450) with GreylistEntry{source, dest, seen_at}
  else: normal delivery
  finally: audit SpamEvaluated{score} (score only, never body)
```

## Sieve compile flow

```
put_filters(mailbox, script):
  reject if script.len() > 64 KiB
  compile via SieveCompiler (Pigeonhole) -> reject on syntax error
  persist SieveScript{active=true}; reload dovecot sieve
```

## Endpoints

```
PUT  /api/v1/mail/domains/{id}/antispam      body { engine, score_threshold, greylist_enabled }
PUT  /api/v1/mail/mailboxes/{id}/filters     body { script }
GET  /api/v1/mail/mailboxes/{id}/filters
PUT  /api/v1/mail/mailboxes/{id}/autoresponder body { subject, body, starts_at?, ends_at? }
PUT  /api/v1/mail/domains/{id}/forwarders    body { forwarders[] }
PUT  /api/v1/mail/domains/{id}/catchall      body { destination }
GET  /api/v1/mail/lists
POST /api/v1/mail/lists                      body { name, moderation }
```

## Tests

```
1.1 Unit: spam score threshold routing; Sieve compile success/failure;
    autoresponder window active check.
1.2 Property: spam audit never contains message body; Sieve script
    cap enforced; forwarder loop detection.
1.3 Service tests w/ mock scorer + mock sieve: antispam policy apply,
    filter apply, autoresponder window.
1.4 Integration: live inbound routes to Spam folder above threshold;
    oversize Sieve rejected.
1.5 CLI E2E: set antispam policy; put sieve; enable autoresponder.
1.6 Web: Mail tabs (CSRF), Sieve editor, autoresponder form.
```
