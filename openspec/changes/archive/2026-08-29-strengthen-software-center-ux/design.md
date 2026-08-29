# Design: Software Center trust and deployment UX

## Explore & Reuse

- Reuse `SoftwareCenterService`, catalog diagnostics, signed manifest/digest verification, install plans, compatibility handlers, job progress, retry, cancel, rollback, and existing card/detail renderers.
- Reuse semantic badges, warning banners, progressbar ARIA, confirmation layer, and UI-state components.
- Preserve fail-closed artifact verification and never hide a blocked reason.

## Detail contract

Every catalog item shows source ID, publisher/signature state, version, license, compatibility, permissions/capabilities, dependencies, conflicts, size estimate, installed state, and last operation. A blocked action explains the exact gate and recovery command/action.

## Transaction UX

Preview is read-only and lists planned files/services/ports plus risk. Execute requires explicit confirmation. Progress shows current step, elapsed/updated state, cancel availability, and correlation ID. Failure shows safe error, retry/rollback options, and preserved logs without secrets.

## Verification

Tests cover signed/unsigned/placeholder/invalid manifests, incompatible components, conflicts, cancellation, retry, rollback, and accessible state announcements.
