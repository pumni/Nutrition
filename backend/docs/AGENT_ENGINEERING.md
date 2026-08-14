# Agent Engineering Operating Model

This repository uses human Task Intent, a trusted compiler, and a policy-bounded implementation agent.

## Human authority

Human owners decide product and nutrition-domain semantics, architecture boundaries, public API contracts, database and migration intent, dependency strategy, security/privacy policy, LLM/provider trust boundaries, infrastructure, behavior-version semantics, canonical publication, and release policy. Those decisions are represented by Task Intent approvals, ADRs, foundation decisions, invariants, and risk policy.

## Agent autonomy

The agent reads the minimal modules selected by `.agent/context/router.json`, investigates the repository, forms and revises a durable plan, chooses implementation files and order, implements within the scope envelope, runs canonical gates, and self-corrects fixable failures. Machine policies and gate registries are queried when needed; they are not default prompt context.

## Task, scope, and risk

Human intent contains the objective, acceptance criteria, non-negotiables, coarse scope hints, and explicit protected approvals. The trusted compiler binds the current baseline, scope ceiling, approvals, and risk floor. The agent discovers relevant modules during read-only investigation; verification derives modules and atomic gates from the actual diff. Risk describes blast radius; authorization is separate: `unprotected`, `approved_protected_change`, or `requires_human_decision`. Agent execution state may raise risk with repository evidence but never lower the compiled floor.

## Verification and evidence

The trusted runner owns executable gate definitions and writes bounded verification evidence outside the target worktree. Implementation reports contain only gate IDs, statuses, and evidence references. Behavioral evals run real adapter trials in disposable worktrees; graders inspect final environment state rather than trusting self-reported success.

## Freshness and maintenance

The source register records artifact-to-authoritative-source relationships. Human-curated context requires review when an authoritative source changes; deterministic artifacts use their own `--check` or verifier when a real consumer exists. The active context graph contains current operating guidance only; completed migration material is historical and is not a current source or route.
