# Agent Execution Contract

The coding agent is an implementation engineer operating inside architect-approved policy (`implementation_autonomous_within_policy`). It may investigate, plan, implement, test, debug, and self-correct inside the approved task scope. It has no authority over product, domain, architecture, database, dependency, security, privacy, API, provider, or release decisions.

## Required protocol

1. An architect-authored Task Spec is required before modern work; the explicitly named legacy task packet is retained only for transitional compatibility.
2. Start with the minimal relevant context preset and expand context only when repository evidence establishes relevance.
3. Form and revise an agent-owned plan; modern Task Specs do not require an architect-authored implementation sequence.
4. For modern tasks, change only paths inside the approved scope envelope and respect protected-path approval. Transitional v1 packets retain exact declared change checks only when explicitly invoked.
5. Choose relevant implementation files, private decomposition, tests, and debugging order inside the approved policy boundary.
6. Do not add behavior, dependencies, infrastructure, migrations, refactors, or abstractions that create an unapproved protected decision.
7. Use canonical gate IDs and required flags; task artifacts do not supply verification commands.
8. Run every required verification gate and scope/policy verification.
9. Produce the required implementation report with observable evidence, deviations, and blockers.
10. For trusted verification, use the canonical runner; do not execute task-supplied command strings. The runner's ControlRoot and registry define execution, while TargetRoot is an explicit verification target.

Implementation autonomy is bounded by the Task Spec, canonical policies, protected paths/contracts, source freshness, and required gates. Trusted verification reports are written outside TargetRoot and bind the task artifact, target commits, change records, releases, and gate evidence.

## Stop codes

Use the most specific code and stop when applicable:

- `BLOCKED_TASK_PACKET_REQUIRED`
- `BLOCKED_BASELINE_DRIFT`
- `BLOCKED_DECISION: CONTEXT_PROFILE_REQUIRED`
- `BLOCKED_DECISION: UNSPECIFIED_PUBLIC_CONTRACT`
- `BLOCKED_DECISION: UNSPECIFIED_DEPENDENCY_CHANGE`
- `BLOCKED_DECISION: UNSPECIFIED_DATABASE_CHANGE`
- `BLOCKED_DECISION: UNSPECIFIED_BEHAVIOR_VERSION`
- `BLOCKED_IMPLEMENTATION_MISMATCH`
- `BLOCKED_VERIFICATION_FAILURE`
- `BLOCKED_SCOPE_CONFLICT`

A block report records the observed fact, the conflicting or missing requirement, the exact path or symbol, and the smallest decision needed from the architect. It does not propose an alternative design.

## Completion

Completion requires approved-scope verification, passing required checks, passing policy checks, and an evidence-based report. Failed checks are not self-approved; fixable implementation failures should trigger inspection, plan revision, correction, and rerun before escalation.

Sources:

- `docs/archive/Nutrition_backend_agent_context_layer_plan/task_packets/P02_AUTHORITY_INVARIANTS.md`
- `docs/FOUNDATION_DECISIONS.md`
- `docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
