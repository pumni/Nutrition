# 04 — Context Model and Loading Rules

## Principle

Context is a dependency graph, not a single giant prompt.

A task packet identifies exactly one context profile. The profile expands to a small ordered list.

## Universal boot sequence

Every coding task follows this exact sequence:

```text
1. Read AGENTS.md.
2. Read .agent/manifest.json.
3. Read current task packet.
4. Validate packet with scripts/verify-agent-context.ps1.
5. Read authority/executor-contract.md.
6. Read context files listed by the packet's context profile, in profile order.
7. Inspect only the implementation files required by the packet.
8. Execute implementation_sequence exactly.
9. Run packet verification gates.
10. Run ACL changed-path verification against the task packet.
11. Produce implementation report.
```

The executor must not "browse the repository for inspiration" before packet validation.

## Core context

Every profile includes:
- `.agent/authority/executor-contract.md`
- `.agent/authority/decision-policy.md`
- `.agent/invariants/architecture.md`
- `.agent/invariants/security-privacy.md`
- `.agent/maps/crate-map.json`
- `.agent/maps/change-impact-map.json`
- `.agent/maps/verification-map.json`

Profiles then add only task-specific context.

## What belongs in an invariant file

Only stable, high-cost-to-violate truths:
- no LLM nutrient invention;
- domain purity;
- append-only revisions;
- published immutability;
- no raw meal logging;
- hosted parser fail-closed;
- forward-only migrations;
- no infrastructure without ADR/evidence.

Do not put:
- transient issue descriptions;
- implementation TODOs;
- line-by-line code explanations;
- model prompting tricks;
- style preferences already enforced by formatter/linter.

## What belongs in a context pack

A context pack explains:
- module responsibility;
- important entrypoints;
- state transitions;
- ports/contracts;
- data ownership;
- high-risk invariants;
- source references;
- which verification catches errors.

It does not reproduce full source files.

## Reading order inside profiles

1. authority
2. cross-cutting invariants
3. module context
4. maps/impact
5. source documents named by the task packet if deeper detail is required
6. implementation files

This prevents large blueprint documents from entering context unless required.

## Context profile cannot be inferred by executor

Even if a path clearly suggests a profile, the executor must not pick it.

Why:
- profile selection is part of task framing;
- it determines which invariants/gates matter;
- letting the executor choose lets it silently under-load context.

The architect can use `change-impact-map.json` to decide the profile when authoring the packet.

## Maximum profile rule

A normal task may use one primary profile plus `core`.

If a task genuinely spans multiple major modules, the architect should normally split it into separate packets. A multi-profile packet is allowed only if explicitly authored and must list all profiles in execution order.

ACL v1 templates use a single `context_profile` to encourage small packets.

## Context freshness

Before implementation:
- ACL verifier validates source-lock;
- executor checks baseline;
- task packet may declare a required repository commit.

If either is stale, stop rather than guessing which context is current.
