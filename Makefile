# OpenPanel quality gate.
#
# The Makefile is the *manager*: it knows the checks that exist and in
# what order they run, but the actual work lives in one small script per
# concern under `scripts/` (fmt, clippy, docs, audit, tests, coverage).
#
# Entry points:
#   make check     — run every quality gate in order (CI entry point)
#   make fmt       — format gate only
#   make clippy    — lint gate only
#   make docs      — doc-link gate only
#   make audit     — dependency audit only (skipped if tool absent)
#   make test      — full test suite (delegates to scripts/check-tests.sh)
#   make coverage  — informational coverage report
#
# Every per-check script prints `step: <name> status: ok | failed` and
# exits non-zero on failure; `make` short-circuits on the first one.

.PHONY: check fmt clippy docs audit test coverage

check: fmt clippy docs audit test
	@echo ""
	@echo "=== All quality checks passed ==="

fmt:
	@scripts/check-fmt.sh

clippy:
	@scripts/check-clippy.sh

docs:
	@scripts/check-docs.sh

audit:
	@scripts/check-audit.sh

test:
	@scripts/check-tests.sh

coverage:
	@scripts/coverage.sh
