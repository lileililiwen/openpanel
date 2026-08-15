# Add Database Privilege Management — Design

## DbGrant model

```rust
pub enum GrantScope {
    Global,                       // *.* 
    Database(DatabaseId),         // db.*
    Table(DatabaseId, String),    // db.table
}

pub struct DbGrant {
    pub user_id: DbUserId,
    pub scope: GrantScope,
    pub privileges: Vec<Privilege>,  // SELECT, INSERT, ...
    pub with_grant_option: bool,
}
```

## RemoteAccess model

```rust
pub struct RemoteAccess {
    pub db_id: DatabaseId,
    pub enabled: bool,
    pub bind: IpAddr,             // 0.0.0.0 only if opt-in
    pub acl: Vec<Cidr>,           // allowed source ranges
    pub allow_any_source: bool,   // explicit owner opt-in
}
```

## AdminToolSession model

```rust
pub struct AdminToolSession {
    pub db_id: DatabaseId,
    pub tool: AdminTool,          // PhpMyAdmin | PgAdmin
    pub signed_token: SignedToken, // short-lived, single-use
    pub launched_by: UserId,
}
```

## Flows

```
set_privileges(db_id, uid, grants):
  validate scopes resolve inside db_id (table must belong to db)
  diff against current grants; apply REVOKE + GRANT
  audit DbPrivilegesChanged{uid, before, after}   // privileges only
  reject global scope unless caller is Owner

set_remote_access(db_id, enabled, acl, allow_any_source):
  if enable and acl empty:
    if allow_any_source -> bind 0.0.0.0, acl=[0.0.0.0/0]
    else -> reject (no open-by-default)
  apply bind + ACL to engine; audit RemoteAccessChanged{db_id, acl}

launch_admin_tool(db_id, tool):
  token = identity.sign_short_lived(db_id, launched_by)
  audit AdminToolLaunched{db_id, tool}
  return SSO redirect into the tool container
```

## Endpoints

```
GET  /api/v1/databases/{id}/users/{uid}/privileges
PUT  /api/v1/databases/{id}/users/{uid}/privileges  body { grants[] }
PUT  /api/v1/databases/{id}/remote-access
     body { enabled, acl[], allow_any_source? }
POST /api/v1/databases/{id}/admin-tool  body { tool }
```

## Tests

```
1.1 Unit: grant-scope resolution (table belongs to db); ACL default
    rule; SSO token expiry/single-use.
1.2 Property: privileges never leak across database boundaries; remote
    enable with empty ACL + no opt-in is always rejected.
1.3 Service tests w/ mock engine: grant, revoke, remote toggle, tool
    launch; audit events recorded (privilege names only).
1.4 Integration: applied GRANT visible in engine; 0.0.0.0/0 requires
    opt-in; SSO token grants tool access.
1.5 CLI E2E: openpanel db user grants -> remote -> tool.
1.6 Web: user editor, remote toggle with opt-in warning, tool button.
```
