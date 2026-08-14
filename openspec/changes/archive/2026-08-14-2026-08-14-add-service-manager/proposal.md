# Add Service Manager

## Why

There is **no service-manager UI** in OpenPanel to start, stop,
restart, enable, or disable system services such as `nginx`,
`php-fpm`, `mysql`, or `redis`. Operators today must SSH in and run
`systemctl` by hand, losing auditability and a safe, role-gated
surface. This change adds a `service-manager` bounded context guarded
by the Admin role.

## What Changes

- New bounded context `service-manager` carrying the `ServiceControl`
  aggregate and `ServiceLister`, `ServiceActor`.
- New endpoints: `GET /admin/services`,
  `POST /admin/services/{name}/{action}` where `action` is one of
  `start`, `stop`, `restart`, `enable`, `disable`.
- List services with status + recent logs; perform guarded lifecycle
  actions. All actions are Admin-gated and audited.

## Capabilities

### New Capabilities

- `service-manager`: list system services with status and recent logs,
  and start/stop/restart/enable/disable them, guarded by the Admin
  role.

## Impact

- Domain: `ServiceInfo`, `ServiceAction`, `ServiceStatus`.
- App: `ServiceLister`, `ServiceActor`.
- API/CLI/web: `/admin/services*`, web Admin → Services panel.
- Security: every action is Admin-gated and audited; service names are
  validated against an allow-list (no arbitrary unit names from UI).
- Coupling: extends `software-center` management surface; status feeds
  the host view in `monitoring`.
