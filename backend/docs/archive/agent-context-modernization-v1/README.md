# Agent Context Modernization Guide

Status: **architect-approved migration guidance**  
Repository: `pumni/Nutrition_backend`  
Target: modern frontier coding-agent workflow with strict human decision authority and outcome-based verification.

## Purpose

This directory defines the approved modernization of the repository's coding-agent context layer.

The target is **not** an unrestricted autonomous coding system. The target is a system in which:

- architecture, product, domain, security, privacy, API, persistence, dependency, provider, behavior-version, and release decisions remain human/architect decisions;
- the coding agent receives those decisions as constraints and refactors the repository accordingly;
- the coding agent may investigate implementation details, locate affected files, form an implementation plan, implement, test, debug, and self-correct inside the approved decision boundary;
- deterministic verification enforces invariants and outcomes instead of prescribing the agent's reasoning trajectory.

This guide is authoritative for the modernization work itself. It does not supersede existing product/domain/security decisions unless a document here explicitly says that a context-layer rule is being replaced.

## Core modernization principle

The existing context layer is strong in invariants, provenance, verification, and trusted execution. Its primary weakness is that it also acts as a remote control for the coding agent's implementation reasoning.

The modernization changes the model from:

```text
Architect decides WHAT + HOW + WHERE + ORDER
Coding agent performs an exact implementation sequence
Verifier checks exact trajectory compliance
```

into:

```text
Architect decides WHAT + WHY + NON-NEGOTIABLES + RISK BOUNDARY
Coding agent investigates HOW + WHERE + ORDER
Verifier checks OUTCOME + INVARIANTS + SCOPE + EVIDENCE
Architect resolves protected/high-impact decisions
```

## Non-negotiable authority rule

The coding agent does **not** make project decisions.

It may make implementation choices only when those choices do not create or alter a protected decision.

Protected decisions include:

- product and nutrition-domain semantics;
- architecture boundaries;
- public API contracts;
- database schema and migration intent;
- dependency additions/removals with architectural impact;
- security and privacy behavior;
- hosted-provider policy;
- canonical data publication policy;
- behavior-version semantics;
- production infrastructure and release policy.

If implementation requires one of these decisions and the approved migration packet does not already decide it, the coding agent must stop and report the decision required. It may explain evidence and consequences, but it must not redefine the project architecture.

## Documents

1. [`01_DECISIONS.md`](01_DECISIONS.md) — architectural decisions for this modernization.
2. [`02_TARGET_ARCHITECTURE.md`](02_TARGET_ARCHITECTURE.md) — target `.agent` model and responsibility boundaries.
3. [`03_MIGRATION_PLAN.md`](03_MIGRATION_PLAN.md) — ordered migration phases and exit criteria.
4. [`04_REFACTOR_PACKETS.md`](04_REFACTOR_PACKETS.md) — implementation packets for the coding agent.
5. [`05_AGENT_EXECUTION_CONTRACT.md`](05_AGENT_EXECUTION_CONTRACT.md) — what the coding agent may and may not decide while refactoring.
6. [`06_BEHAVIORAL_EVALS.md`](06_BEHAVIORAL_EVALS.md) — new eval suite for agent quality, not only governance integrity.
7. [`07_COMPLETION_CRITERIA.md`](07_COMPLETION_CRITERIA.md) — repository-level definition of done.

## Rules for executing this guide

- Follow phases in `03_MIGRATION_PLAN.md` in order unless an explicit dependency permits parallel work.
- Do not redesign the target architecture while implementing it.
- Do not weaken existing product/domain/security/privacy invariants.
- Do not remove deterministic verification before replacement verification exists.
- Do not merge migration steps that make failures difficult to attribute.
- Each refactor packet must leave the repository verifiable.
- Behavioral evals must be established before autonomy-related legacy constraints are finally removed.

## Desired end state

At completion, `.agent/` should function as:

```text
repository constitution
+ machine-readable policy
+ generated/discoverable repository maps
+ risk-adaptive context routing
+ trusted verification
+ behavioral evals
+ durable task state
```

It should no longer function as a step-by-step implementation script for the model.
