---
name: openpanel-forge-delivery
description: Use when Forge hands a project to OpenPanel for staged deployment, health-gated promotion, rollback, or publish-provider integration, including the subsequent Hermora enrollment boundary.
---

# OpenPanel delivery for Forge

OpenPanel owns deployment operations and the optional Forge publish provider.
Read `Agents.md`, `HANDOFF.md`, `openspec/specs/forge-publish-provider/spec.md`,
the application-delivery and deployment-adapter specs, and current provider
code before asserting an operation is available. Forge coordinates the
workflow; Hermora owns post-publish enrollment and site operations.

## Delivery path

1. Check the target project, artifact or Git revision, release evidence,
   declared adapter capability, destination, credentials reference, and
   operator authority. Treat dry-run or a planned deployment as planning
   evidence only.
2. Use the versioned Forge publish-provider contract and OpenPanel's existing
   application delivery, audit, idempotency, and rollback behavior. Do not
   duplicate deployment logic in Forge or add a source dependency between
   repositories.
3. Stage the release, inspect the actual health result, and require explicit
   promotion. Keep preflight refusal, transport failure, failed health,
   partial deployment, and failed rollback distinguishable. Record the exact
   revision, operation id, and provider outcome.
4. After a successful deployment record, enroll in Hermora through its own
   scoped machine interface. Retry enrollment independently without
   redeploying; an OpenPanel success does not prove Hermora readiness.
5. For OpenPanel source changes, follow `Agents.md`'s design review, local
   Gate, OpenSpec, archive, and two-commit workflow. Provider integration tests
   do not replace a controlled real-adapter or runtime check.

Use the current owner docs for command syntax and capability status. Never
report a deployed site from scaffold, build, strict spec validation, or a
simulated provider response alone.
