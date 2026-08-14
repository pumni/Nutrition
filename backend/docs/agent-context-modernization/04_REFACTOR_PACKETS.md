# 04 — Refactor Packets

These packets are implementation instructions for AI coding agents. The architecture decisions are already made in this guide. The coding agent investigates repository details and performs the refactor; it does not redesign the target.

Each packet should be executed separately unless dependencies make a small combination obviously safer.

---

# Packet M01 — Baseline evidence

## Objective

Capture the current context-layer behavior before modernization.

## Allowed work

- add baseline documentation/fixtures/eval metadata;
- run current verification;
- record current release IDs and context budgets.

## Do not change

- authority semantics;
- task schema;
- verifier behavior;
- runtime product behavior.

## Acceptance criteria

- current ACL/runner/CI/foundation checks have recorded results;
- current context/source-lock coverage is recorded;
- representative behavioral tasks are enumerated.

---

# Packet M02 — Canonicalize provenance paths

## Objective

Make every context source reference resolve to a real repository path.

## Required outcome

Any `Sources:` or `Derived from:` path used as provenance points to the exact canonical repository path.

## Expected areas

- `.agent/invariants/**`;
- `.agent/contexts/**`;
- `.agent/maps/source-register.json`;
- associated tests.

## Prohibited decisions

Do not change product/domain/security semantics while fixing provenance.

## Verification

- no referenced provenance path is missing;
- ACL self-tests pass;
- foundation verification passes.

---

# Packet M03 — Source-lock v2

## Objective

Remove the hard-coded partial source-lock model.

## Required outcome

The verifier derives freshness coverage from declared source dependencies rather than an exact hard-coded list of eight sources.

## Required invariant

For every derived context artifact `A` and declared source `S`:

```text
change(S) without corresponding valid refresh(A) => verification failure
```

## Implementation freedom

The agent chooses the internal data structure and helper functions, provided:

- source-register or its approved replacement remains canonical;
- output is deterministic;
- paths are normalized;
- hashing remains SHA-256 unless existing architecture requires another already-approved algorithm;
- missing sources fail closed.

## Do not decide

- new provenance policy;
- new hash algorithm;
- weakening freshness validation.

## Verification

Add positive and negative self-test cases for code-source drift, doc-source drift, missing sources, and deterministic lock generation/validation.

---

# Packet M04 — Canonical verification truth

## Objective

Remove disagreement between natural-language required checks and the machine gate registry.

## Required outcome

Every mandatory executable check has one canonical gate ID.

## Work

- inventory all `Required gates:` language;
- map required checks to existing gate IDs;
- add missing canonical gates where the requirement is already architect-approved;
- otherwise reword non-executable/non-mandatory guidance so it is not represented as a gate.

## Do not decide

Do not invent new release requirements. If documentation implies a new mandatory release policy not already approved, stop and report it.

---

# Packet M05 — Report contract cleanup

## Objective

Make trusted verification the sole owner of executable command definitions.

## Required outcome

Implementation reports reference verification by gate/result/evidence identity rather than embedding agent-owned command truth.

## Compatibility

Migration may support both old and new report versions temporarily.

## Verification

- schema positive/negative fixtures;
- trusted runner report validation;
- legacy compatibility test if still supported.

---

# Packet M06 — Task Spec v2

## Objective

Introduce the modern human-authored task contract.

## Required fields

- objective;
- acceptance criteria;
- risk level;
- scope envelope;
- protected boundaries;
- required policy/context modules;
- required verification gates;
- explicitly approved protected decisions when applicable;
- baseline binding.

## Explicitly remove from default human planning responsibility

- implementation sequence;
- predicted exact changed-file sets;
- forced empty decision-point list.

## Do not remove v1 yet

This packet introduces v2 side-by-side.

---

# Packet M07 — Agent Plan and execution state

## Objective

Create an agent-owned, mutable implementation plan and durable progress state.

## Required separation

```text
Task Spec = architect authority
Agent Plan = implementation hypothesis
Execution State = progress/evidence
```

The agent plan must never override task spec or canonical policies.

## Privacy requirement

Do not store secrets, authorization headers, raw meal text, hosted model raw payloads, or sensitive analysis content.

---

# Packet M08 — Risk policy

## Objective

Encode risk-adaptive autonomy.

## Required risk classes

- `investigation`;
- `normal_internal`;
- `boundary_sensitive`;
- `protected_decision`.

## Protected decision domains

At minimum:

- product/domain behavior;
- architecture;
- public API;
- database/migration intent;
- security/privacy;
- behavior-version semantics;
- production provider/infrastructure;
- canonical publication;
- release policy;
- architecturally significant dependency changes.

## Required behavior

A task cannot silently downgrade its own risk classification.

---

# Packet M09 — Context presets and router

## Objective

Convert exclusive profiles into progressive context presets/modules.

## Required outcome

A task can begin with a minimal relevant preset and add other modules when investigation establishes relevance.

## Preserve

- context budgets;
- risk tags;
- mandatory gates;
- protected boundaries.

## Required capabilities

- path → relevant policies/context/gates;
- risk → mandatory policies/gates;
- multi-module composition;
- compatibility alias for old profile names during migration.

---

# Packet M10 — Scope envelope verifier

## Objective

Allow implementation discovery without losing scope control.

## Default rule

```text
actual changed paths ⊆ approved scope envelope
```

plus protected-path enforcement.

## Exact path mode

Retain an optional exact-file mode only for explicitly high-risk/mechanical tasks.

## Required evals

- legitimate unexpected file inside scope passes;
- unexpected file outside scope fails;
- protected path fails without approval;
- explicit protected-path approval passes;
- create/modify/delete are accurately reported after implementation.

---

# Packet M11 — Behavioral eval harness

## Objective

Implement the suite defined by `06_BEHAVIORAL_EVALS.md`.

## Critical property

Evaluate observable behavior and repository result. Do not require or inspect hidden chain-of-thought.

## Required comparison mode

The harness should be able to compare legacy constrained execution and modern policy-bounded execution where practical.

---

# Packet M12 — Remove mandatory architect implementation sequence

## Preconditions

- M11 behavioral evals exist;
- scope-envelope verification is operational;
- protected-decision detection is tested.

## Objective

Move implementation sequencing into the agent plan.

## Required behavior

The agent may revise its plan after failed tests or new repository evidence without requiring the architect to rewrite the task spec.

---

# Packet M13 — Authority contract v2

## Objective

Replace mechanical-executor semantics with policy-bounded implementation autonomy.

## Required agent permissions

Inside approved policy/scope, agent may:

- investigate;
- search beyond the initial preset when relevant;
- choose implementation files;
- choose local/private design;
- add appropriate tests;
- debug failures;
- revise its plan;
- perform semantics-preserving or acceptance-required internal refactors.

## Required prohibitions

Agent may not independently change protected decisions.

## Block behavior

When a protected decision is needed, report:

- observed fact;
- affected constraint;
- evidence/location;
- impact on implementation;
- smallest architect decision required.

The agent must not treat its preferred architecture as approved.

---

# Packet M14 — AGENTS.md modernization

## Objective

Make `AGENTS.md` a short navigation/authority entrypoint.

## It should communicate

- architect owns protected decisions;
- investigate before changing code;
- start with minimal relevant context;
- expand context based on evidence;
- follow canonical policies;
- use canonical verification;
- escalate protected decisions.

## It should not contain

- detailed implementation sequences;
- exact file predictions;
- redundant copies of domain/security docs;
- provider/model-specific prompting tricks;
- instructions that force the model to ignore relevant repository evidence.

---

# Packet M15 — Generated repository facts

## Objective

Reduce hand-maintained duplication of discoverable implementation facts.

## Candidates

- crate graph from Cargo metadata/workspace configuration;
- source dependency index;
- test inventory;
- route/schema inventory where deterministic generation is available.

## Rule

Do not generate policy. Generate facts; humans author policy.

---

# Packet M16 — Legacy removal

## Preconditions

- all migration completion criteria pass;
- behavioral eval comparison shows no material policy regression;
- modern contracts are exercised in representative tasks.

## Remove/deprecate

- default exact-diff prediction requirement;
- mandatory human implementation sequence;
- exclusive architect-selected context profile requirement;
- mechanical-executor language;
- prohibition on relevant context expansion;
- obsolete v1-only schema fields after compatibility window ends.

## Preserve

Everything listed in `03_MIGRATION_PLAN.md` under “Do not remove”.
