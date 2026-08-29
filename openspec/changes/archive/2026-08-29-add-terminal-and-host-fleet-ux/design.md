# Design: Terminal and host-fleet UX

## Explore & Reuse

- Reuse `web_terminal` domain/app/API ports and existing terminal integration tests.
- Reuse agent registration, mTLS, host SSH-key lifecycle, service manager, logs, monitoring, and audit ports.
- Reuse shell layout, job/loading components, confirmation layer, and status tokens.

## Session model

A terminal session has host ID, authenticated actor, working directory scope, created/last activity timestamps, connection state, and expiry. The browser receives no private key material. Session termination is explicit and automatic on timeout/logout.

## UX and safety

Host context is always visible. Commands that can stop/restart the panel or host require an explanatory confirmation and use the existing service actions where possible. Output is bounded, streamed with accessible status, and never mixed with another host. Disconnected sessions show reconnect/terminate actions.

## Verification

Test host isolation, session expiry, reconnect, output truncation, keyboard access, role restrictions, audit linkage, and secret non-leakage. E2E tests must use a fake terminal/agent port, never a real host.
