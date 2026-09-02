# Design: Make governance regression checks mandatory

## Explore & Reuse

- Reuse `scripts/test-gates.sh`, which already exercises tasks ordering,
  layering, reuse, and spec-test drift.
- Reuse Makefile targets and `scripts/lib/step.sh`; do not duplicate gate
  logic in YAML.
- Reuse `.github/workflows/ci.yml` and the existing Rust setup/cache steps.

## Approach

Add `make test-gates` and include it in `make check` after the script-level
governance checks. Change CI’s required check job to run `make check`, remove
`continue-on-error` from OpenSpec validation, and use strict mode where the
archived policy promises strict enforcement. Keep coverage informational as
the existing quality spec allows.

The self-test will be deterministic and fixture-only. CI remains read-only
with respect to the repository and will fail when the Makefile, gate wiring,
or a gate’s positive/negative behavior regresses.

## Verification

Run `make test-gates`, inspect `make -n check`, parse CI YAML, and run strict
OpenSpec validation. A negative fixture must prove each tested gate fails for
the prohibited input.

## Approval gate

Implementation requires explicit human approval of this design before
`apply`.
