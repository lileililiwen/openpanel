# Refine Databases with remote access enforcement — Design

## Explore & Reuse

- `crates/openpanel-domain/src/db_privileges/mod.rs:147–187` —
  `RemoteAccess` CIDR list + `0.0.0.0/0` opt-in gate (reused as-is).
- `crates/openpanel-app/src/db_privileges/service.rs:94–153` —
  `RemoteAccessController` + `DbRemoteAccessChanged` audit (extended,
  not duplicated).
- `mysql` CLI detection + shell-out conventions from
  `openspec/specs/databases/spec.md` ("MySQL Provisioning via Shell")
  and its app implementation — the grant port uses the same binary
  discovery and error mapping (`MysqlMissing` → 503).
- Admin-tool SSO token issuer (`AdminToolSession`, same module) is
  untouched; this change only makes the network ACL real.

## Host-pattern derivation (pure)

```rust
pub fn mysql_host_pattern(cidr: IpCidr) -> Result<String, DbError> {
    // /32 -> "203.0.113.7"; /24 -> "203.0.113.%";
    // IPv6 /128 -> "2001:db8::1", /64 -> "2001:db8:0:0:%"
    // reject prefix < /16 (too broad) unless global opt-in flag set
}
```

MySQL cannot express arbitrary CIDRs; coarse prefixes map to `%`
wildcards. The derivation is pure and property-tested; the service
stores the applied pattern alongside the CIDR so reconcile is exact.

## Reconcile flow

```
apply(remote_access):
   desired = {("user","localhost")} ∪ {(user, pattern) for cidr in acl}
   current = SHOW GRANTS / mysql.user rows for user
   diff -> CREATE/DROP USER + GRANT ALL ON db.* TO user@pattern
   audit DbRemoteAccessChanged{added, removed}
boot():
   reconcile all DBs with non-empty ACLs (self-heal manual drift)
```

Failures leave state untouched (apply-all-or-nothing per database) and
return `DatabaseError::GrantFailed{step}` mapped to 502 at the API.

## MODIFIED requirements

Two live requirements change minimally:

1. *Database Aggregate*: `db_host` becomes "`localhost` by default;
   additional remote host patterns are derived from the database's
   RemoteAccess ACL".
2. *MySQL Provisioning via Shell*: create additionally provisions
   `user@'localhost'` plus one account per applied remote pattern;
   drop removes all accounts for the user.

Full modified text is in `specs/databases/spec.md`.

## Endpoints / CLI / Web

```
GET/PUT /api/v1/databases/{id}/remote-access   {cidrs[], enabled}
CLI: openpanel database remote-access {show,add,remove,disable}
Web: Databases → Remote Access card (CIDR table, opt-in warning)
```
