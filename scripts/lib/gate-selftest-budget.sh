#!/usr/bin/env bash
# scripts/lib/gate-selftest-budget.sh — budget + count assertions for
# `scripts/test-gates.sh`. Sourced by the self-test; the functions
# below are small enough to be tested in isolation by the
# `gate-selftest-budget` fixture group inside `test-gates.sh` itself.
#
# Per the `Gate Self-Test Budget Compliance` quality requirement
# (P1+; see openspec/specs/quality/spec.md), the suite MUST time
# itself and, when a positive budget is configured via
# OPENPANEL_GATE_SELFTEST_BUDGET_SECS, exit non-zero when the budget
# is exceeded. A budget of `0` or unset (the cold-CI default) degrades
# the same measurement to an advisory `slow-test-gates: <n>s` line.
# The suite also MUST keep reporting its self-test count, and the
# count MUST NOT drop below the pinned minimum
# (GATE_SELFTEST_MIN_PASS_COUNT, default 118) without a reviewed spec
# change.
#
# Environment:
#   OPENPANEL_GATE_SELFTEST_BUDGET_SECS — positive: hard fail when
#       exceeded; 0 or unset: advisory only.
#   GATE_SELFTEST_START — the epoch-second timestamp at suite start
#       (set by the suite). Defaults to `date +%s` so the function
#       is callable from any context.
#   GATE_SELFTEST_SLOWEST_GROUP — the slowest fixture group name;
#       printed on both fail and advisory paths so a regression is
#       attributable.
#   GATE_SELFTEST_MIN_PASS_COUNT — the pinned minimum pass count the
#       suite must report (default 118).
#
# The functions never modify the tree. They print to stderr so the
# collector (the suite) can interleave or suppress as needed.
#
# All times use epoch seconds (date +%s) so the function works
# consistently across subshells (where $SECONDS resets). The unit
# under test is wall-clock seconds.

# gate_selftest_check_budget: assert the elapsed wall time against
# the configured budget. Accepts an optional `now` argument (epoch
# seconds) for testability; defaults to `date +%s`.
gate_selftest_check_budget() {
    local budget="${OPENPANEL_GATE_SELFTEST_BUDGET_SECS:-0}"
    local start="${GATE_SELFTEST_START:-$(date +%s)}"
    local now="${1:-$(date +%s)}"
    local slowest="${GATE_SELFTEST_SLOWEST_GROUP:-unknown}"
    local elapsed=$((now - start))
    if [ "${budget}" -gt 0 ] 2>/dev/null; then
        if [ "${elapsed}" -gt "${budget}" ]; then
            echo "gate-selftest-budget: elapsed ${elapsed}s exceeds budget ${budget}s (slowest: ${slowest})" >&2
            return 1
        fi
        echo "gate-selftest-budget: elapsed ${elapsed}s within budget ${budget}s (slowest: ${slowest})" >&2
    else
        echo "slow-test-gates: ${elapsed}s" >&2
    fi
    return 0
}

# gate_selftest_check_count: assert the suite's pass count is at or
# above the pinned minimum. Returns 0 on pass, 1 when the count
# regressed below the minimum.
gate_selftest_check_count() {
    local actual_count="$1"
    local min_count="${GATE_SELFTEST_MIN_PASS_COUNT:-118}"
    if [ "${actual_count}" -lt "${min_count}" ] 2>/dev/null; then
        echo "gate-selftest-budget: pass count ${actual_count} below pinned minimum ${min_count} (decrease requires a reviewed spec change)" >&2
        return 1
    fi
    return 0
}
