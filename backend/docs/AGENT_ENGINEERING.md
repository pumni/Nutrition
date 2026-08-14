# Agent Engineering Operating Model

This repository uses a human-owned Task Spec and a policy-bounded implementation agent.

## Human authority

Human owners decide product and nutrition-domain semantics, architecture boundaries, public API contracts, database and migration intent, dependency strategy, security/privacy policy, LLM/provider trust boundaries, infrastructure, behavior-version semantics, canonical publication, and release policy. Those decisions are represented by approved Task Specs, ADRs, foundation decisions, invariants, and risk policy.

## Agent autonomy

The agent reads the minimal context selected by `.agent/context/router.json`, investigates the repository, forms and revises a durable plan, chooses implementation files and order, implements within the scope envelope, runs canonical gates, and self-corrects fixable failures. The agent does not invent protected decisions or task commands.

## Task, scope, and risk

Task Spec v2 contains typed risk, canonical protected domains, required policy modules, required gate IDs, an explicit scope envelope, and structured approval references. Risk describes blast radius. Authorization is separate: `unprotected`, `approved_protected_change`, or `requires_human_decision`.

## Verification and evidence

The trusted runner owns executable gate definitions and writes bounded verification evidence outside the target worktree. Implementation reports contain only gate IDs, statuses, and evidence references. Behavioral evals run real adapter trials in disposable worktrees; graders inspect final environment state rather than trusting self-reported success.

## Freshness and maintenance

Declared source dependencies are hashed with SHA-256. Generated facts and derived context artifacts must be regenerated when their inputs change. The active context graph contains current operating guidance only; completed migration material is historical archive and is not a current source or route.
