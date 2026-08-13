## ADDED Requirements

### Requirement: Typed Rule Set

The system SHALL let an Owner attach a `RuleSet` to a site. A ruleset is a versioned, ordered list of typed rules (`RateLimit`, `ConnLimit`, `GeoBlock`, `UserAgentBlock`, `PathBlock`, `HeaderChallenge`, `BodySizeCap`) and a `default_action` (`Allow` | `Challenge` | `Deny`). Unknown rule kinds and unknown fields MUST be rejected at the API.

#### Scenario: Add a rate-limit rule

- **WHEN** an Owner posts a `RateLimit` rule with `{ zone, rate, burst, nodelay? }`
- **THEN** the ruleset is updated, the snippet is recompiled, `nginx -t` passes, and the new limit is live.

#### Scenario: Unknown field rejected

- **WHEN** a rule body contains a field that is not in the schema for its kind
- **THEN** the panel returns 422 and the ruleset is not modified.

### Requirement: Snippet Compilation is Pure and Stable

The compiler SHALL be a pure function of the ruleset: identical rulesets produce byte-identical snippets. The compiled snippet is inlined into the per-site nginx config before any `location` block. The panel SHALL run `nginx -t` against the rendered file before reloading; on failure the previous config is restored and a `WafCompileError` is returned.

#### Scenario: Compile failure rollback

- **WHEN** the compiled snippet fails `nginx -t`
- **THEN** the previous config is restored, the new ruleset is rejected, and the audit log records the failure with a redacted reason.

#### Scenario: Idempotent recompile

- **WHEN** the same ruleset is recompiled after no other change
- **THEN** the snippet is byte-identical and `nginx -t` is not required to run again.

### Requirement: Per-Rule Hit Metrics

The panel SHALL sample per-rule hit counts and last-triggered timestamps and SHALL publish them as `waf.hits{kind, site, action}` time-series rows. The notification pipeline (separate capability) can subscribe to spike alerts.

#### Scenario: Hit increments

- **WHEN** a request matches a `PathBlock` rule and the action is `Deny`
- **THEN** the rule's hit counter increments by 1 and the last-triggered timestamp updates.

#### Scenario: Spike alert

- **WHEN** the rate of `waf.hits` over the last 60 s exceeds the configured threshold
- **THEN** the monitoring module records an `AlertFired` event consumable by the notification pipeline.

### Requirement: Dry-Run Rule Test

`POST /api/v1/sites/{id}/waf/test` SHALL accept a single rule and return the compiled snippet and the simulated matching behaviour for a small set of canonical request fixtures. The dry-run MUST NOT modify the ruleset or touch the live config.

#### Scenario: Dry-run matches

- **WHEN** an Owner submits a `PathBlock` rule and a fixture request that matches the pattern
- **THEN** the response shows the compiled snippet and `{ would_match: true, action: "Deny" }`.

### Requirement: WAF Surfaces

REST, CLI, and `/sites/{id}/waf` web surfaces SHALL support ruleset get, put, hit read, and dry-run test. Browser mutations MUST enforce CSRF. A `PUT` is atomic: the entire ruleset is replaced, the snippet is recompiled, and the existing rollback discipline applies.

#### Scenario: Owner replaces a rule set through a supported surface

- **WHEN** an Owner replaces a site's complete rule set through REST, CLI, or the web form
- **THEN** the panel validates the typed document, compiles and tests the complete candidate, atomically activates it, and exposes the same persisted rule set and hit totals through every surface; a browser request with a missing or invalid CSRF token is rejected before mutation.
