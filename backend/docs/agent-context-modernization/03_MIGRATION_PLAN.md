# 03 — Migration Plan

The migration is intentionally staged. Do not perform a single large rewrite of `.agent/`.

Each phase must preserve a runnable/verifiable repository and leave enough evidence to determine whether the next phase is safe.

---

# Phase 0 — Baseline and freeze the current behavior

## Goal

Capture the current governance and verification baseline before changing agent autonomy semantics.

## Required work

- record the current `.agent` release identifiers;
- run the current ACL self-tests;
- run trusted runner self-tests;
- run CI policy verification;
- run foundation verification;
- record current context byte sizes;
- record current source-register/source-lock coverage;
- identify representative coding tasks for later behavioral evals.

## No architecture changes in this phase

This phase is evidence collection only.

## Exit criteria

- current governance tests are reproducibly green;
- baseline evidence is stored or referenced;
- at least 10 representative agent tasks are identified.

---

# Phase 1 — Fix truth/freshness inconsistencies before changing autonomy

## Goal

Make the existing context layer internally coherent.

## Required changes

### 1.1 Canonical provenance paths

Normalize every `Sources:` / `Derived from:` path to an actual repository-resolvable path.

Known target pattern:

```text
docs/archive/nutrition_backend_blueprint_v1.0/...
```

Do not preserve shorthand paths that do not exist in the repository.

### 1.2 Source-lock completeness

Replace the hard-coded exact eight-file source lock model.

The new lock must derive its source set from the canonical source dependency registry.

Required property:

```text
if artifact A declares source S
and S changes
and A is unchanged/stale,
context freshness verification fails.
```

Include code sources used by compiled context files, not only documentation sources.

### 1.3 One truth for mandatory gates

Reconcile natural-language `Required gates:` with machine-readable profiles and the gate registry.

Mandatory executable checks must have canonical gate IDs.

### 1.4 Report contract consistency

Agent-owned implementation reports should reference canonical gate results, not redefine executable commands.

## Exit criteria

- source register and source lock agree;
- source freshness covers declared source-code dependencies;
- no context file references a nonexistent provenance path;
- every mandatory executable requirement has a gate ID;
- governance self-tests cover these cases.

---

# Phase 2 — Introduce v2 contracts without removing v1 enforcement

## Goal

Create the modern contract model side-by-side with the current one.

## Add

- `task-spec` v2 schema;
- `agent-plan` schema;
- execution-state schema or equivalent durable state model;
- risk-policy machine-readable artifact.

## Task Spec v2 minimum fields

```text
schema_version
task_id
objective
acceptance_criteria
risk_level
scope_envelope
protected_boundaries
required_policy_modules
required_verification_gates
approved_protected_decisions
```

## Explicitly not mandatory in v2

```text
implementation_sequence
exact create_files
exact modify_files
exact delete_files
empty decision_points
```

## Agent Plan minimum fields

```text
task_id
baseline
observations
hypothesis
planned_changes
verification_strategy
risks
protected_decisions_required
status
```

## Exit criteria

- schemas exist;
- verifier can validate v2 artifacts without changing current production governance behavior;
- sample v2 task and plan fixtures pass validation;
- invalid protected-decision cases fail.

---

# Phase 3 — Add agent behavioral evals

## Goal

Create safety evidence before removing legacy cognitive constraints.

Implement the eval categories specified in `06_BEHAVIORAL_EVALS.md`.

Minimum initial suite:

- 3 context-discovery cases;
- 3 root-cause/scope cases;
- 3 invariant-preservation cases;
- 2 missing-decision/escalation cases;
- 2 verification-recovery cases;
- 2 context-efficiency cases.

The suite may use fixtures/synthetic repository states where deterministic evaluation is easier.

## Exit criteria

- at least 15 behavioral cases exist;
- eval results distinguish success from policy violation;
- eval harness can compare legacy and modern execution modes;
- eval output is reproducible enough for migration decisions.

---

# Phase 4 — Replace exclusive profiles with progressive context routing

## Goal

Stop requiring the architect to select exactly one exclusive context profile.

## Required changes

- convert profiles into presets/modules;
- add machine-readable routing from paths/risk/domain to policies/context/gates;
- allow multiple modules for cross-cutting tasks;
- allow agent investigation to expand context when evidence requires it;
- preserve context budget and relevance discipline.

## Compatibility

Existing profile names may remain as aliases during migration.

## Exit criteria

- current task classes can be routed without manual profile selection;
- cross-cutting tasks can combine modules;
- context loading remains bounded;
- behavioral context-discovery evals pass.

---

# Phase 5 — Move from exact diff prediction to scope envelopes

## Goal

Allow the coding agent to discover the actual implementation files.

## Required changes

Default verification changes from:

```text
actual create/modify/delete == architect predeclared sets
```

to:

```text
actual changed paths are inside approved scope
AND protected paths are untouched unless explicitly approved
AND relevant impact/policy checks pass
```

## Keep exact-file restrictions only when

- risk policy explicitly requires them;
- changing control-plane/security-sensitive artifacts;
- a packet is intentionally mechanical.

## Exit criteria

- representative tasks can discover unpredicted but legitimate files;
- scope-violation evals still fail closed;
- protected paths remain enforceable;
- no regression in governance evals.

---

# Phase 6 — Remove architect-authored implementation sequence

## Goal

Stop prescribing the model's implementation trajectory.

## Required changes

- task specs no longer require `implementation_sequence`;
- agent plan owns implementation sequencing;
- plan can change when verification or investigation disproves the initial hypothesis;
- final report records meaningful deviations from the agent's own plan when relevant.

## Required safeguard before removal

Behavioral evals for:

- root cause;
- minimal scope;
- recovery;
- invariant preservation;
- protected decision detection

must be operational.

## Exit criteria

- modern mode performs at least as well as legacy mode on policy correctness;
- task success does not regress materially;
- agent can recover from an initially incorrect plan without architect re-authoring a packet.

---

# Phase 7 — Replace `implementation_only` identity with policy-bounded implementation autonomy

## Goal

Update authority semantics to reflect the target model.

## New conceptual role

```text
implementation_autonomous_within_policy
```

The exact field name may differ, but the semantics are fixed.

## Agent may decide

- how to investigate;
- which relevant implementation files inside scope require change;
- local/private design;
- internal decomposition;
- test additions;
- debugging sequence;
- refactoring required to satisfy approved behavior without creating protected decisions.

## Agent may not decide

Protected project decisions listed in `01_DECISIONS.md`.

## Exit criteria

- authority docs no longer describe the agent as a mechanical executor;
- protected decisions still fail closed;
- blocked reports identify the smallest human decision required;
- behavioral evals pass.

---

# Phase 8 — Add durable work state for long-running refactors

## Goal

Make execution state independent from chat history.

## Minimum durable state

- task spec reference/hash;
- baseline commit;
- current agent plan;
- discoveries;
- progress;
- verification references;
- protected-decision blockers.

## Restrictions

- durable work state is not a new source of product truth;
- secrets/raw meal content must not be stored;
- stale work state must be detectable against repository baseline.

## Exit criteria

- interrupted work can resume from repository state plus durable task artifacts;
- stale baseline is detected;
- no sensitive content leakage is introduced.

---

# Phase 9 — Remove obsolete compatibility machinery

## Goal

Delete legacy fields/rules only after modern replacements are proven.

Candidates for removal:

- v1 mandatory implementation sequence;
- v1 exact architect-predicted changed-file sets as default;
- architect-selected single context profile requirement;
- `decision_points maxItems=0` semantics;
- instructions forbidding relevant context expansion;
- mechanical-executor wording.

## Do not remove

- stable domain/security/privacy/architecture invariants;
- trusted runner;
- canonical gate registry;
- provenance/freshness verification;
- CI trust boundaries;
- behavior versioning;
- protected-decision escalation.

## Exit criteria

See `07_COMPLETION_CRITERIA.md`.

---

# Migration policy

For every phase:

1. establish observable acceptance criteria;
2. implement the smallest coherent change;
3. run governance verification;
4. run applicable behavioral evals;
5. inspect failures;
6. correct implementation without changing approved architecture;
7. only then proceed.
