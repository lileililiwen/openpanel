# site-staging delta

## ADDED Requirements

### Requirement: Staging Mail Capture Toggle

Each staging slot SHALL carry a mail-capture toggle (default off). When
enabled, outbound mail originating from the slot's site is diverted to
the staging-mail-capture inbox instead of the production mail path, and
capture state is visible on the slot view.

#### Scenario: Toggle defaults off

- **WHEN** a staging slot is created
- **THEN** mail capture is disabled and site mail flows unchanged.

#### Scenario: Destruction purges capture data

- **WHEN** a staging slot with capture enabled is destroyed
- **THEN** all captured messages for that slot are deleted.
