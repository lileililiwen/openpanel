# Add Service Manager — Design

## ServiceInfo model

```rust
pub struct ServiceInfo {
    pub name: String,            // e.g. "nginx", "mysql"
    pub status: ServiceStatus,   // Running | Stopped | Failed | Unknown
    pub enabled: bool,           // boot-enabled via systemd
    pub recent_logs: Vec<LogLine>, // last N journal lines
}
```

## Action flow

```
list_services():
  query systemd for allow-listed units
  map -> Vec<ServiceInfo{status, enabled, recent_logs}>

perform(name, action):
  if !allow_list.contains(name) -> ServiceError::NotAllowed
  if !actor.is_admin          -> ServiceError::Forbidden
  systemctl <action> <name>   (allow-listed verb only)
  audit ServiceAction{name, action, by}
```

## Endpoints

```
GET  /api/v1/admin/services
POST /api/v1/admin/services/{name}/{action}   action in {start,stop,restart,enable,disable}
```

## Tests

```
1.1 Unit: allow-list reject; status mapping from systemd state.
1.2 Property: action verb always maps to allow-listed systemctl call.
1.3 Service tests w/ mock systemctl: list, start, restart, enable.
1.4 Integration: real systemctl lists units; non-admin denied.
1.5 Web: Services panel lists status + action buttons + logs.
```
