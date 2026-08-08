## ADDED Requirements

### Requirement: Dashboard Landing Page

The `web-ui` shell SHALL render a dashboard at `/` for authenticated users.
The dashboard SHALL show:

- a CPU gauge, a memory gauge, a disk gauge (highest mount percent), and a
  load figure, sourced from the current host snapshot;
- quick-count cards for Sites, Databases, Files, SSL certificates, and Users,
  each linking to its resource page;
- a recent-alerts panel listing the latest alert events, with an empty state
  when no alerts exist.

#### Scenario: Authenticated user opens the dashboard

- **WHEN** a user with a valid session requests `/`
- **THEN** the server renders the dashboard inside the shell with the four
  host gauges, the five quick-count cards, and the recent-alerts panel.

#### Scenario: Host gauges refresh

- **WHEN** a partial request hits `GET /dashboard/gauges`
- **THEN** the server returns an HTML fragment with freshly collected CPU /
  memory / disk / load values that HTMX swaps into place.

### Requirement: Dashboard Empty States

The dashboard SHALL render gracefully when there is nothing to show: zero
counts display `0` on the cards, and the alerts panel shows a "No alerts"
message rather than an error or blank region.

#### Scenario: Fresh install

- **WHEN** a fresh install has no sites, databases, or alerts and the owner
  opens the dashboard
- **THEN** the cards show `0`, the alerts panel shows the empty state, and the
  page renders without error.
