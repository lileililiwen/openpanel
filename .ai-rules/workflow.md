# Project Workflow

Use BFS → DFS → BFS. Map impact and evidence first, implement one coherent change, then re-audit callers, persistence, APIs, tests, security, and release boundaries.

Run the shared governance checker and the project verification command before archive or completion. Run `node scripts/check-openspec-change-names.mjs` from the governance repository before OpenSpec status/instructions.

Each completed OpenSpec spec/change requires exactly two commits: first the implementation/tests/archive commit, then a HANDOFF.md-only pointer/evidence commit. Stop after commit 2.
