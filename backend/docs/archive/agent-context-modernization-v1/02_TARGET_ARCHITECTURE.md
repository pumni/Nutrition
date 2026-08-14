# 02 — Target Architecture

## Target operating model

The target system separates four responsibilities:

```text
Human / Architect
    │
    │ owns project decisions
    ▼
Task Spec + Policies
    │
    ▼
Coding Agent
    │ investigates / plans / implements / self-corrects
    ▼
Trusted Verification
    │ deterministic evidence + invariant enforcement
    ▼
Completion / Escalation
```

The coding agent is not an architect. It is an autonomous implementation engineer inside architect-approved policy.

## Target `.agent/` responsibilities

The final tree may evolve during implementation, but responsibility separation is fixed:

```text
.agent/
├── policies/
│   ├── architecture.md
│   ├── product-domain.md
│   ├── security-privacy.md
│   ├── llm-boundary.md
│   └── data-replay.md
│
├── generated/
│   ├── crate-graph.json
│   ├── source-index.json
│   ├── change-impact-map.json
│   └── test-map.json
│
├── context/
│   ├── presets.json
│   └── router.json
│
├── verification/
│   ├── gates.json
│   └── risk-policy.json
│
├── contracts/
│   ├── task-spec.schema.json
│   ├── agent-plan.schema.json
│   ├── execution-state.schema.json
│   └── verification-report.schema.json
│
├── evals/
│   ├── governance/
│   └── agent-behavior/
│
└── state/
    └── source-lock.json
```

This is a responsibility map, not a requirement to rename every current directory in one commit. Migration packets define the safe transition.

## Repository entrypoint

`AGENTS.md` remains short. Its purpose is:

1. identify repository policy sources;
2. establish human decision authority;
3. tell the agent to investigate before choosing implementation;
4. point to context routing and canonical verification;
5. define escalation for protected decisions.

It must not become a procedural implementation manual.

## Human-authored Task Spec

A Task Spec defines:

- task ID;
- objective;
- acceptance criteria;
- approved behavior decisions;
- risk classification;
- scope envelope;
- protected paths/contracts;
- required policy modules;
- mandatory verification gates;
- explicit human-approved high-impact decisions, if any.

A Task Spec does **not** normally define:

- implementation sequence;
- exact private helper design;
- exact changed-file list;
- exact debugging path;
- exact order of tests beyond required final gates.

## Agent-owned Plan

After read-only investigation, the coding agent creates or updates an implementation plan containing:

- observed repository state;
- implementation hypothesis;
- expected affected files;
- implementation steps;
- verification strategy;
- discovered risks;
- protected decisions that would block progress.

The plan is mutable working state. It is not project authority.

## Context routing

Context loading follows progressive disclosure:

```text
base policies
    +
initial preset selected from task/path/risk
    +
additional context discovered from evidence
```

The router may use:

- requested task domain;
- affected path predictions;
- crate graph;
- risk tags;
- source register;
- discovered dependencies;
- changed paths during implementation.

The agent may inspect beyond the initial preset when needed to establish correctness, but it must not load unrelated context without reason.

## Stable policy vs generated facts

### Stable policy

Human-curated and reviewed:

- product/domain rules;
- architectural boundaries;
- security/privacy constraints;
- LLM boundary;
- data replay and immutability;
- protected decision categories.

### Generated facts

Generated/verified from repository state where practical:

- workspace/crate membership;
- dependency graph;
- key source/test paths;
- source hashes;
- verification inventory;
- route or schema inventories when tooling exists.

Generated artifacts must carry provenance and freshness evidence.

## Scope model

Default scope model:

```text
actual changes ⊆ approved scope envelope
actual changes ∩ protected paths = ∅
```

unless a protected path is explicitly approved by the task spec.

Exact changed-file declarations may be used only when the risk policy explicitly requires them.

## Verification model

Verification is layered:

### Layer A — repository hygiene

- formatting;
- linting;
- compilation/tests;
- schema parsing.

### Layer B — domain/system behavior

- PostgreSQL integration;
- replay/immutability;
- API smoke contracts;
- parser schema/grounding;
- benchmark gates when applicable.

### Layer C — policy verification

- protected boundaries;
- sensitive logging rules;
- source freshness;
- dependency restrictions;
- behavior-version requirements.

### Layer D — agent behavior evals

- context selection;
- root-cause quality;
- scope quality;
- recovery;
- escalation;
- efficiency.

## Risk-adaptive execution

### Risk 0: read-only investigation

Agent may search, inspect, read, and run safe non-mutating diagnostics.

### Risk 1: normal internal refactor/fix

Agent may plan and modify any path in the approved subsystem envelope, subject to policies and tests.

### Risk 2: boundary-sensitive work

Agent may implement already-approved behavior across boundaries but cannot change those boundaries without explicit approval.

### Risk 3: protected decisions

Work stops until architect approval explicitly decides the protected change.

## Long-running execution state

For long refactors, state must survive conversation/session boundaries.

Required durable concepts:

```text
approved spec
current plan
observations/discoveries
progress
verification evidence
blockers / protected decisions
```

This state must not become a second source of project truth. Project decisions remain in canonical docs/task specs/ADRs.

## Completion model

A coding task is complete when:

- acceptance criteria are satisfied;
- actual diff stays inside approved scope;
- protected decisions were not silently made;
- required verification passes;
- relevant invariants remain true;
- agent report references trusted verification evidence;
- no unresolved high-impact decision remains.
