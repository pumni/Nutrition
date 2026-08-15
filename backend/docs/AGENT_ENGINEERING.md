# Agent Engineering Operating Model

This repository uses human Task Intent, a trusted compiler, and a policy-bounded implementation agent.

## Human authority

Human owners decide product and nutrition-domain semantics, architecture boundaries, public API contracts, database and migration intent, dependency strategy, security/privacy policy, LLM/provider trust boundaries, infrastructure, behavior-version semantics, canonical publication, and release policy. Those decisions are represented by Task Intent approvals, ADRs, foundation decisions, invariants, and risk policy.

## Agent autonomy

The agent reads the minimal modules selected by `.agent/context/router.json`, investigates the repository, plans and replans as needed, chooses implementation files and order, implements within the scope envelope, runs canonical gates, and self-corrects fixable failures. Machine policies and gate registries are queried when needed; they are not default prompt context.

## Task, scope, and risk

Human Task Intent contains the objective, acceptance criteria, non-negotiables, a non-empty coarse write boundary, and explicit protected approvals. Use `**` only when repository-wide mutation is intentional. The trusted prepare phase binds the baseline, scope ceiling, approvals, and risk floor into a compiled Task Spec. The agent discovers relevant modules and owns implementation sequencing during read-only investigation; verification derives modules, gates, and effective risk from the actual diff. Human-approved means Task Intent and protected approvals; machine-bound means the compiled execution envelope; observed means the final diff; verifier-derived means gates and risk.

## Verification and evidence

The trusted runner consumes the prepared Task Spec, validates the baseline-to-target relationship, owns executable gate definitions, and writes bounded verification evidence outside the target worktree. Implementation reports contain only gate IDs, statuses, and evidence references. Behavioral evals run real adapter trials in disposable worktrees; graders inspect final environment state rather than trusting self-reported success.

## Freshness and maintenance

The source register records artifact-to-authoritative-source relationships. Human-curated context requires review when an authoritative source changes; deterministic artifacts use their own `--check` or verifier when a real consumer exists. The active context graph contains current operating guidance only; completed migration material is historical and is not a current source or route.
