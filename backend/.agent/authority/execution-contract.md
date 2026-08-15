# Agent Execution Contract

The implementation agent works inside a machine-bound Task Spec prepared from human Task Intent and current repository policy. It may investigate, form and revise a plan, implement, test, debug, and self-correct within the approved scope. Human authority remains exclusive for product/domain semantics, architecture boundaries, public contracts, database intent, dependencies, security/privacy, provider trust, infrastructure, behavior versions, publication, and release decisions.

## Required protocol

1. Read the compiled Task Spec, the minimal routed context, and the canonical policies before writing.
2. Choose and revise implementation sequencing from repository evidence; persist a plan only when task complexity, handoff, or context continuity makes it useful. The Task Spec defines outcomes and constraints, not an implementation trajectory.
3. Change only paths inside the scope envelope and respect protected-path authorization.
4. Choose relevant files, private decomposition, tests, and debugging order inside the policy boundary.
5. Use canonical gate IDs; task artifacts and reports refer to gates and evidence but never define commands.
6. Run required verification, scope, and policy checks.
7. Produce an observable implementation report with acceptance evidence, gate references, deviations, and blockers.

## Protected blockers

Use the most specific classification:

- `PROTECTED_DECISION_REQUIRED`
- `BASELINE_STALE`
- `SCOPE_VIOLATION`
- `POLICY_CONFLICT`
- `VERIFICATION_FAILED`
- `CONTEXT_INTEGRITY_FAILED`

A blocker records the observed fact, exact repository evidence, the applicable constraint, why the approved task cannot proceed, and the smallest human decision required. Fixable implementation failures should trigger inspection, plan revision, correction, and rerun before escalation.

## Completion

Completion requires passing applicable canonical gates, approved-scope verification, policy checks, and an evidence-based report. Trusted evidence is written by the canonical runner outside the target worktree.
