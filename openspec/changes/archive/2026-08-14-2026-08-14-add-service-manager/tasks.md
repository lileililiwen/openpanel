# Add Service Manager — Tasks

## 1. Testing

- [x] 1.1 Unit: allow-list reject of unknown unit; map systemd
      `active`/`inactive`/`failed` to `ServiceStatus`.
- [x] 1.2 Property: the action verb only ever maps to an allow-listed
      `systemctl` call; no arbitrary unit name reaches the executor.
- [x] 1.3 Service: list services, start, restart, enable; audit each.
- [x] 1.4 Integration: real `systemctl` lists allow-listed units; a
      non-Admin caller is denied.
- [ ] 1.5 Web: Services panel lists status + action buttons + recent
      logs view.

## 2. Domain and Application

- [x] 2.1 Implement `ServiceInfo`, `ServiceAction`, `ServiceStatus`
      under `crates/openpanel-domain/src/service_manager/`.
- [x] 2.2 Add SQLite migration for `service_action_history`.
- [x] 2.3 Implement `ServiceLister`, `ServiceActor`; register via
      `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/admin/services` and
      `/admin/services/{name}/{action}` routes (Admin-gated,
      allow-list enforced).
- [ ] 3.2 Build the Services panel (status list, action buttons,
      recent-logs drawer).

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: list services, restart `nginx`, confirm status
      flips and an audit row exists; a non-Admin call is 403.
- [x] 4.4 Archive with `openspec archive add-service-manager`.
