# agent-quality Specification

Cross-cutting guardrails that keep AI coding agents aligned with
OpenPanel's global architecture and prevent hallucinated, duplicated,
or architecturally-drifted code. This is the **top-level** capability
for agent behaviour; `quality` and `testing` operationalise it. The
gaps this capability closes are the system's key loss when relying on
agents: loss of global view, architectural drift, and failure to reuse
existing logic.

## ADDED Requirements

### Requirement: Project Context Injected Into Every Artifact

OpenSpec's `config.yaml` MUST declare a `context:` block describing
the tech stack, layering, conventions, and domain, and `rules:` blocks
for each artifact type (`proposal`, `tasks`, `specs`, `design`). The
`openspec context` command MUST report the declared context (not
"No references declared"). This context SHALL be the first thing an
agent sees when creating or applying any artifact, so it cannot
proceed without the global view.

#### Scenario: Context is present

- **WHEN** a developer runs `openspec context`
- **THEN** the output includes the project's tech stack, layering, and
  conventions — not the empty "No references declared" message.

#### Scenario: Artifact creation carries the global view

- **WHEN** an agent proposes a change
- **THEN** the `rules:` for `proposal` remind it to cite an existing
  module it will reuse and to respect DDD layering.

### Requirement: Single Canonical Agent Contract Shared By Every Runtime

A canonical `AGENTS.md` (generated/kept in sync via `openspec update`)
MUST exist at the repo root and MUST be the single source of agent
guidance. Every agent runtime directory (`.agents/`, `.codex/`,
`.qoder/`) MUST reference or symlink to it so Codex, generic agents,
and Qoder all load the identical contract. Divergence where only one
runtime has skills is forbidden.

#### Scenario: Every runtime loads the contract

- **WHEN** an agent starts in Codex, a generic agent, or Qoder
- **THEN** it loads the same `AGENTS.md`/`Agents.md` contract and the
  `agent-quality` spec as its first context.

### Requirement: Explore And Reuse Before Implement

The agent workflow (§8 of `Agents.md`) MUST include an explicit
`## 0. Explore & Reuse` step that runs **before** planning or
implementing. In this step the agent MUST:
- produce a repo-map (`scripts/repo-map.sh`) of the modules and public
  APIs relevant to the task,
- search the codebase for existing utilities, traits, repository
  implementations, and modules that already satisfy the requirement,
- record, in `design.md`, the specific existing code it will reuse and
  why new code is necessary.

New code that duplicates an existing, reusable utility is a violation
of this requirement and MUST be rejected at review.

#### Scenario: Reuse is demonstrated before new code

- **WHEN** an agent implements a feature that overlaps an existing
  module (e.g. a repository, a service, a crypto helper)
- **THEN** `design.md` names the existing code and explains reuse; the
  agent does not introduce a second implementation of the same logic.

### Requirement: Repo-Map Tooling Available

The repository SHALL ship `scripts/repo-map.sh`, which prints a
structural map of crates, modules, and their public APIs (functions,
structs, traits) so an agent can discover existing logic without
reading every file. The tool MUST be referenced from `Agents.md` and
MUST degrade gracefully when `cargo` is unavailable.

#### Scenario: Map produced

- **WHEN** a developer or agent runs `scripts/repo-map.sh`
- **THEN** it prints a tree of `crates/*/src/**` public items grouped
  by module, without modifying any source file.

### Requirement: Context Compaction Discipline

Agents MUST keep their working context focused. The workflow SHALL
require distilling progress to a short plan/`progress.md` after each
verified phase (research → plan → implement) so the context window
does not fill with noise that causes mid-task hallucination. The
`openspec-explore` skill already models "think, then capture" — this
requirement makes compaction an explicit, required step.

#### Scenario: Compaction after a phase

- **WHEN** an agent finishes the plan phase of a change
- **THEN** it writes a concise status (goal, approach, done, current
  blocker) to the change folder before beginning implementation.

### Requirement: Human Review Of Design Before Apply

No change SHALL be implemented (`apply`) until its `design.md` has been
reviewed and approved by a human principal. Reviewing the *research and
plan* — not just the resulting code — catches architectural drift and
hallucinated assumptions before they propagate into the tree.

#### Scenario: Apply is gated

- **WHEN** an agent finishes `propose`/`design` but `design.md` is not
  approved
- **THEN** the workflow blocks `apply` and the change stays in
  `openspec/changes/` unmerged.
