# forge-publish-provider Specification

## ADDED Requirements

### Requirement: The provider SHALL serve JSON-RPC over newline-delimited stdio (STD-001)

The `openpanel-cli forge-provider` subcommand MUST run a stdio loop that
decodes one frame per stdin line using the existing `forge.publish` codec,
dispatches valid requests through the `ForgePublishProvider` facade,
writes exactly one encoded response line per request (none for
notifications), and emits JSON-RPC error frames −32700/−32601/−32602 for
unparseable lines, wrong methods, and invalid envelopes respectively without
terminating the loop.

#### Scenario: Valid publish request round-trips

- **WHEN** the provider receives a conforming `forge.publish` request line
- **THEN** stdout yields exactly one response line for that request id
  within the 0.1.0 contract shape and an audit outcome is recorded

#### Scenario: Garbage line does not kill the provider

- **WHEN** stdin delivers a malformed JSON line followed by a valid request
- **THEN** the provider answers the malformed line with a parse-error frame
  and still serves the following request

#### Scenario: Clean EOF

- **WHEN** stdin closes after the last request
- **THEN** the provider stops its lifecycle handle and exits successfully

### Requirement: The provider SHALL bind an operator caller and refuse unsafe startup (STD-002)

Startup MUST require `OPENPANEL_FORGE_PUBLISH_PROVIDER` enabled and
`OPENPANEL_FORGE_PUBLISH_OPERATOR` naming an existing user whose role
passes the facade's Owner/Admin gate; any unmet precondition MUST cause a
non-zero config-exit before any stdin read, leaving OpenPanel itself fully
operational.

#### Scenario: Toggle unset

- **WHEN** the enable variable is absent or invalid
- **THEN** the subcommand exits with the config error code, reads no stdin,
  and the rest of OpenPanel is unaffected

#### Scenario: Under-privileged operator

- **WHEN** the named operator resolves to a role below Admin
- **THEN** startup is refused with a message naming the failure kind, not
  secret material

### Requirement: Error-frame emission SHALL be part of the public crate surface (STD-003)

`openpanel-app` MUST re-export the envelope decoder and error-frame encoder
used by the transport so integrators can emit protocol-level errors without
reaching into module internals.

#### Scenario: Transport composes only public items

- **WHEN** the CLI handler is built against `openpanel-app` public exports
- **THEN** all loop primitives (decode, encode response, encode error,
  envelope types, lifecycle handle) resolve from the crate root
