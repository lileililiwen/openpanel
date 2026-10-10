# capability-navigation Specification

## ADDED Requirements

### Requirement: Terminal Page Is Discoverable From The Shell

The `/terminal` page MUST have one registry entry (`key:
"terminal"`, route `/terminal`, capability `terminal`, label
`Terminal`, scope Global, `min_role: Owner`) in the Operations
group, mounted in the web router, with role filtering as
defense in depth: the link hides below `Owner` and direct
access still returns the authorization response.

#### Scenario: Owner sees the Terminal link

- **WHEN** an Owner loads the shell with the `terminal`
  capability mounted
- **THEN** the Operations group renders the `Terminal` link
  resolving to the mounted `/terminal` route.

#### Scenario: User is denied direct access

- **WHEN** a User-role caller requests `/terminal` directly
- **THEN** the handler denies the request without leaking
  server credentials or command bodies.
