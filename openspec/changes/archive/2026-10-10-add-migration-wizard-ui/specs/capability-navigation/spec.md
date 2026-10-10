# capability-navigation Specification

## ADDED Requirements

### Requirement: Migration Wizard Is Discoverable From The Shell

The `/migrate` wizard MUST have one registry entry (`key:
"migrate"`, route `/migrate`, capability `migration`, label
`Migrate`, scope Global, `min_role: Owner`) in the Operations
group, mounted in the web router, with role filtering as
defense in depth: the link hides below `Owner` and direct
access still returns the authorization response.

#### Scenario: Owner sees the Migrate link

- **WHEN** an Owner loads the shell with the `migration`
  capability mounted
- **THEN** the Operations group renders the `Migrate` link
  resolving to the mounted `/migrate` route.

#### Scenario: User is denied direct access

- **WHEN** a User-role caller requests `/migrate` directly
- **THEN** the handler denies the request without leaking plan
  or bundle content.
