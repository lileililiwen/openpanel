# Proposal: Make governance regression checks mandatory

## Why

The archived quality and agent-context changes describe executable gate
coverage, but `scripts/test-gates.sh` is orphaned (`make test-gates` fails), CI
does not run the required `make check`, and CI allows OpenSpec validation to
fail. A later change can therefore bypass already-positive governance checks.

## What

Wire the gate self-tests into Make, run the complete quality entry point in
CI, and make strict governance modes explicit for new-change validation.
Reuse all existing gate scripts, `scripts/test-gates.sh`, the Makefile
dispatcher, and the current CI toolchain.

## Capabilities

### New

- `quality`: mandatory governance self-test and CI enforcement.

### Modified

- `testing`: executable verification of gate behavior.

## Non-goals

- No product behavior or Rust domain changes.
- No removal of the documented warning-only policy for pre-existing debt.
- No network or release changes.
