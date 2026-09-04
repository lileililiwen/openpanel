# OpenPanel quality gate.
#
# The Makefile is the *manager*: it knows the checks that exist and in
# what order they run, but the actual work lives in one small script per
# concern under `scripts/` (fmt, clippy, docs, audit, file-length, tests,
# coverage, and the agent-quality gates: tasks-testing-first, reuse,
# layering, spec-test-drift, spec-drift, and the gate self-test).
#
# Entry points:
#   make check     — run every quality gate in order (CI entry point)
#   make fmt       — format gate only
#   make clippy    — lint gate only
#   make docs      — doc-link gate only
#   make audit     — dependency audit gate only (skipped if tool absent)
#   make file-length — per-file line-count gate only (skipped if tool absent)
#   make test      — full test suite (delegates to scripts/check-tests.sh)
#   make test-gates — run scripts/test-gates.sh (the governance self-test)
#   make agent-governance — run scripts/check-agent-governance.sh
#   make governance-contract — run scripts/check-governance-contract.sh
#   make coverage  — informational coverage report
#   make install-lint-tools — install the optional file-length tools
#   make split FILE=<path>  — auto-refactor preview for one file
#   make repo-map  — print a structural map of public APIs (agent aid)
#   make reuse | layering | tasks-testing-first | spec-test-drift | spec-drift — gates only
#
# `make check` gate order (the canonical chain — keep in sync with
# AGENTS.md "Quality gate" line):
#   ensure-lint-tools
#   → fmt → clippy → docs → audit → file-length → scan-literal
#   → tasks-testing-first → reuse → layering
#   → spec-test-drift → spec-drift → agent-governance → governance-contract
#   → test-gates
#   → test
#
# Every per-check script prints `step: <name> status: ok | failed` and
# exits non-zero on failure; `make` short-circuits on the first one.

.PHONY: check fmt clippy docs audit file-length test coverage install-lint-tools ensure-lint-tools split a11y scan-literal tasks-testing-first reuse layering spec-test-drift spec-drift repo-map test-gates agent-governance governance-contract

check: ensure-lint-tools fmt clippy docs audit file-length scan-literal tasks-testing-first reuse layering spec-test-drift spec-drift agent-governance governance-contract test-gates test
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

file-length:
	@scripts/check-file-length.sh

scan-literal:
	@scripts/scan-template-literals.sh

tasks-testing-first:
	@scripts/check-tasks-testing-first.sh

reuse:
	@scripts/check-reuse.sh

reuse-strict:
	@scripts/check-reuse.sh --strict

layering:
	@scripts/check-layering.sh

spec-test-drift:
	@scripts/check-spec-test-drift.sh

spec-drift:
	@scripts/check-spec-drift.sh

repo-map:
	@scripts/repo-map.sh

a11y:
	@echo "a11y: skipping — no dev server is running"
	@echo "  (axe-core is run against the local dev server by .github/workflows/a11y.yml)"

test:
	@scripts/check-tests.sh

test-gates:
	@scripts/test-gates.sh

agent-governance:
	@scripts/check-agent-governance.sh

governance-contract:
	@scripts/check-governance-contract.sh

coverage:
	@scripts/coverage.sh

install-lint-tools:
	@scripts/install-lint-tools.sh

ensure-lint-tools:
	@if command -v cargo-lint-extra >/dev/null 2>&1 && command -v splitrs >/dev/null 2>&1; then \
	  echo "step: lint-tools status: ok"; \
	else \
	  echo "step: lint-tools status: installing"; \
	  $(MAKE) install-lint-tools || echo "step: lint-tools status: skipped (install failed)"; \
	fi

# Auto-refactor preview for one file. Default is `--dry-run`; pass
# `APPLY=1` to perform the split for real.
split:
	@if [ -z "$(FILE)" ]; then \
	  echo "Usage: make split FILE=<path> [APPLY=1]"; \
	  echo "Listing oversized .rs files instead:"; \
	  scripts/install-lint-tools.sh >/dev/null 2>&1 || true; \
	  if command -v cargo-lint-extra >/dev/null 2>&1; then \
	    cargo lint-extra list; \
	  else \
	    find . -name '*.rs' -not -path './target/*' -not -path './node_modules/*' | while read f; do \
	      lines=$$(wc -l < "$$f"); \
	      if [ "$$lines" -ge 850 ]; then \
	        echo "$$f: $$lines lines"; \
	      fi; \
	    done; \
	  fi; \
	else \
	  if [ "$(APPLY)" = "1" ]; then \
	    splitrs "$$FILE"; \
	  else \
	    splitrs --dry-run "$$FILE"; \
	  fi; \
	fi
