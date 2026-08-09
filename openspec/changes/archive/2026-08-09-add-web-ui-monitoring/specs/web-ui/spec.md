## ADDED Requirements

### Requirement: Monitoring History

The `web-ui` SHALL render `GET /monitoring` inside the shell with a metric
selector (`Cpu` / `Memory` / `Disk` / `Network`) and a range selector
(1 h / 6 h / 24 h / 7 d). The selected metric + range SHALL render a
server-rendered SVG sparkline of the samples from `MonitoringService::history`
in `#history-chart`. Empty ranges SHALL render a "no samples" placeholder.

#### Scenario: Viewing metric history

- **WHEN** an authenticated user selects a metric and range on `/monitoring`
- **THEN** the server renders an SVG polyline of the sampled values for that
  metric within the range.

#### Scenario: Empty history

- **WHEN** no samples exist for the selected metric/range
- **THEN** the history fragment renders the "no samples" placeholder.

### Requirement: Alert Feed

The `web-ui` SHALL render the recent alert events on the monitoring page in
`#alert-feed`, sourced from the monitoring alert history, with an empty state
when no alerts exist.

#### Scenario: Alerts displayed

- **WHEN** alert events exist
- **THEN** the feed lists each alert's metric, measured value, and threshold.

#### Scenario: No alerts

- **WHEN** no alert events exist
- **THEN** the feed renders the "No alerts" empty state.
