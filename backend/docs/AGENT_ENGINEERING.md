# Agent Engineering Operating Model

This repository uses a human-owned Task Spec and a policy-bounded implementation agent.

## Human authority

Human owners decide product and nutrition-domain semantics, architecture boundaries, public API contracts, database and migration intent, dependency strategy, security/privacy policy, LLM/provider trust boundaries, infrastructure, behavior-version semantics, canonical publication, and release policy. Those decisions are represented by approved Task Specs, ADRs, foundation decisions, invariants, and risk policy.

## Agent autonomy

The agent reads the minimal modules selected by `.agent/context/router.json`, investigates the repository, forms and revises a durable plan, chooses implementation files and order, implements within the scope envelope, runs canonical gates, and self-corrects fixable failures. Machine policies, gate registries, source locks, and generated indexes are queried when needed; they are not default prompt context.

## Task, scope, and risk

Human intent contains the objective, acceptance criteria, non-negotiables, scope hints, and explicit protected approvals. The trusted compiler derives Task Spec v2 baseline, routed modules, atomic gates, scope authorization, and the risk floor. Risk describes blast radius; authorization is separate: `unprotected`, `approved_protected_change`, or `requires_human_decision`. Agent execution state may raise risk with repository evidence but never lower the compiled floor.

## Verification and evidence

The trusted runner owns executable gate definitions and writes bounded verification evidence outside the target worktree. Implementation reports contain only gate IDs, statuses, and evidence references. Behavioral evals run real adapter trials in disposable worktrees; graders inspect final environment state rather than trusting self-reported success.

## Freshness and maintenance

Declared source dependencies are hashed with SHA-256. Generated facts and derived context artifacts must be regenerated when their inputs change. The active context graph contains current operating guidance only; completed migration material is historical archive and is not a current source or route.
