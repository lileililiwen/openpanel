# Spec delta: quality — gate self-test budget compliance

## ADDED Requirements

### Requirement: Gate Self-Test Budget Compliance

The governance self-test suite `scripts/test-gates.sh` SHALL complete
within driftwatchdog's per-check budget when executed on a warm cache,
so the gate's `test_gates` binding reports PASS and never lands under
"Pending reviews" for reasons the suite itself controls. The suite
MUST measure its own elapsed wall time and, on runs where a positive
budget is configured via `OPENPANEL_GATE_SELFTEST_BUDGET_SECS`, exit
non-zero when the budget is exceeded, printing the elapsed time and
the slowest fixture group. A budget of `0` (or unset on cold CI)
degrades the same measurement to an advisory line. Fixture coverage
MUST NOT be reduced to satisfy the budget: the suite keeps reporting
its fixture and self-test counts, and a decrease requires a reviewed
spec change. This requirement does not amend the protected
`DriftWatchdog Local Gate` requirement, whose pending-review
tolerance for cold cargo builds remains in force.

#### Scenario: Warm gate run reports no pending review

- **WHEN** `driftwatchdog gate` runs on a clean tree with a warm build
  cache
- **THEN** the `test_gates` check reports PASS and no "Pending
  reviews" entry for `test_gates` appears in the run output

#### Scenario: Suite self-censors when it outgrows its budget

- **WHEN** `scripts/test-gates.sh` is run with
  `OPENPANEL_GATE_SELFTEST_BUDGET_SECS` set to a positive value the
  suite exceeds
- **THEN** the suite exits non-zero and prints its elapsed wall time
  and the slowest fixture group

#### Scenario: Cold runs stay advisory

- **WHEN** the suite runs with the budget variable unset or `0`
- **THEN** the suite's pass/fail verdict reflects fixtures only and the
  elapsed time is emitted as an advisory `slow-test-gates:` line at
  most

#### Scenario: Coverage cannot be traded for speed

- **WHEN** the suite's reported fixture or self-test count falls below
  the pinned baseline without a reviewed spec change
- **THEN** the governance self-test fails
