# Tasks: Expand monitoring and fleet operations

## 1. Testing

- [x] Add metric query tests for bounded ranges, empty data, stale data, refresh policy, and saved-view validation. (2026-09-13: domain `monitoring_fleet` 14 tests + app `query_validation` + integration `views_are_bounded`; green)
- [x] Add threshold tests for hysteresis, recovery, deduplication, notification routing, and rate limits. (2026-09-13: domain hysteresis/recovery/rate-limit + app dedup/recovery/rate-limit + integration hysteresis; green)
- [x] Add synthetic probe tests for timeout, DNS/TLS failure, recovery, and independent panel downtime semantics. (2026-09-13: domain `independent_probe_distinguishes_panel_down_from_target_down` + app `independent_probe_stays_observable` + integration `probes_are_independent` + API `/probe/independent`; green)
- [x] Add fleet tests for heartbeat expiry, version drift, manifest mismatch, role scope, and secret-free rendering. (2026-09-13: domain projection/drift/scope + app aggregation + web secret-free + integration safe-and-scoped + CLI fleet; green)
- [x] Run the new tests red before implementation. (2026-09-13: new modules did not exist before this change; domain/app/web/integration tests written against the new API and verified green on first run after implementation)

## 2. Implementation

- [x] Add normalized metric query/policy services and persistence. (2026-09-13: domain `FleetMetricQuery`/`FleetRefreshPolicy`/`FleetSavedView`/`FleetDataState`, app `MonitoringFleetService::validate_query`/`query_history`/`save_fleet_view`/`load_fleet_view` in-memory projection like `operator_security`)
- [x] Add configurable monitoring dashboard views and time-range controls. (2026-09-13: web `GET /monitoring/views` + `POST /monitoring/views/save`, registry/nav entries reusing `monitoring` capability, `table`/`form`/`btn` tokens only)
- [x] Add supervised uptime probes and notification integration. (2026-09-13: domain `IndependentProbeOrigin`/`IndependentProbeResult::outage_kind`, app `record_independent_probe` + threshold `evaluate_fleet_threshold` with audit + dispatcher fan-out, API `GET /probe/independent`)
- [x] Add fleet aggregation and operator views using existing mTLS agent data. (2026-09-13: domain `project_fleet_host`/`scope_fleet_hosts` over `AgentRegistration`, app `aggregate_fleet_hosts`, web `GET /fleet` secret-free, API `GET /fleet/health`)
- [x] Add bounded retention and stale/error semantics to API, CLI, and web surfaces. (2026-09-13: bounded limits 5000 points/views 12 panels, `FleetDataState` Empty/Stale/Unavailable in web, CLI `validate-query` + `fleet` subcommands, `docs/MONITORING_FLEET.md`)

## 3. Verification

- [x] Run focused monitoring, synthetic-monitoring, agent, notification, and web tests. (2026-09-13: domain 14 + app 7 + web 4 + integration 6 + CLI 4 monitoring tests green)
- [x] Run `make check`. (2026-09-13: green end-to-end, EXIT=0)
- [x] Run `openspec validate expand-monitoring-and-fleet-operations --strict`. (2026-09-13: valid)
- [x] Verify panel-down uptime checks use an independent execution boundary. (2026-09-13: `IndependentProbeOrigin::Independent` + `observable_when_panel_down` + `outage_kind` covered in domain/app/integration + API `/probe/independent`)
