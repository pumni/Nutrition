# 01 — Modernization Decisions

These decisions are approved for the agent-context modernization. Coding agents execute them; they do not reopen them during refactoring.

## D1 — Separate decision authority from implementation autonomy

**Decision:** Human/architect authority remains final for protected project decisions. Coding agents gain implementation autonomy only inside already-approved boundaries.

**Reason:** The current layer conflates architectural authority with implementation freedom. Frontier coding agents benefit from investigation, decomposition, local design, debugging, and self-correction, while project-level decisions still require human ownership.

**Consequences:**

- remove `implementation_only` as the conceptual identity of the agent;
- replace it with `implementation_autonomous_within_policy` or equivalent semantics;
- preserve fail-closed behavior when a protected decision is missing.

## D2 — Outcome constraints replace prescribed implementation trajectories

**Decision:** `implementation_sequence` will no longer be a mandatory architect-authored execution contract.

**Reason:** The architect should specify the required result, invariants, protected decisions, scope envelope, and verification. The coding agent should discover the implementation trajectory from the actual repository state.

**Consequences:**

- architect task specs no longer prescribe exact implementation order;
- the coding agent may create/update its own plan after investigation;
- verification focuses on outcomes and policy compliance.

## D3 — Exact predeclared changed-file sets are removed from human task input

**Decision:** Human-authored task specs define a scope envelope and protected paths, not an exact expected diff.

**Reason:** Exact `create_files` / `modify_files` / `delete_files` force the architect to know the implementation answer before the agent investigates the repository.

**Consequences:**

- actual changed files become an agent/report output;
- verifier enforces that actual changes stay inside approved scope;
- sensitive/high-risk packets may still explicitly restrict individual files when required.

## D4 — Stable policy and dynamic implementation facts are different artifact classes

**Decision:** Human-curated invariants/policies remain authoritative. Repository facts that can be derived mechanically should be generated or discovered just in time.

**Stable human-curated examples:**

- architecture invariants;
- nutrition-domain invariants;
- security/privacy policy;
- LLM boundary;
- release and behavior-version policy.

**Generated/discoverable examples:**

- crate graph;
- current key files;
- route map;
- test map;
- dependency graph;
- current source hashes.

## D5 — Context profiles become routing presets, not exclusive information prisons

**Decision:** Existing profiles evolve into presets/context modules. The coding agent starts narrow and may load additional relevant context when repository evidence requires it.

**Reason:** `read only profile files` prevents legitimate investigation and turns context minimization into information prohibition.

**Consequences:**

- profile selection may be automatic;
- multiple modules may be combined for cross-cutting work;
- extra context retrieval must remain task-relevant and auditable.

## D6 — Source freshness must cover every declared source dependency

**Decision:** Source-lock generation derives from the source register or its replacement. Hard-coded partial source lists are removed.

**Reason:** A compiled context artifact must not remain “verified fresh” when one of the code files it claims to summarize has changed.

**Required property:**

```text
source changed + derived context unchanged => freshness verification fails
```

## D7 — Executable requirements have canonical gate IDs

**Decision:** Any requirement described as a mandatory executable check must be represented in the canonical verification registry.

**Reason:** Natural-language “required gates” that have no machine-readable gate create conflicting truths.

**Rule:**

- mandatory executable requirement → canonical gate ID;
- no gate ID → non-mandatory guidance or acceptance criterion.

## D8 — Trusted runner remains authoritative for command execution

**Decision:** Keep canonical gate resolution in the trusted runner/control plane. Task specs and agent reports do not inject arbitrary shell commands.

**Reason:** This is a strong security and reproducibility property of the current design.

## D9 — Governance evals and agent-behavior evals are separate suites

**Decision:** Existing deterministic ACL/runner/CI evals remain. Add a new suite that evaluates observable coding-agent behavior.

**Behavioral dimensions include:**

- relevant context discovery;
- root-cause quality;
- minimal scope;
- invariant preservation;
- missing-decision detection;
- recovery after verification failure;
- escalation quality;
- context efficiency.

## D10 — Autonomy is risk-adaptive

**Decision:** Autonomy increases with reversibility and decreases with blast radius.

### Risk 0 — Investigation

Read/search/inspect/run safe verification. No architectural approval required.

### Risk 1 — Normal internal implementation

Agent may choose files, local design, tests, and implementation plan inside approved boundaries.

### Risk 2 — Boundary-sensitive implementation

Agent may investigate and implement approved behavior, but must escalate if work requires changing a protected contract.

### Risk 3 — Protected decision

Human decision is required before changing public API, migration intent, architecture, dependencies with architectural impact, security/privacy, behavior-version semantics, canonical publication, production provider/infrastructure, or release policy.

## D11 — Blocked agents provide evidence, not alternative architecture decisions

**Decision:** When blocked, the agent reports evidence, impact, the smallest missing decision, and implementation consequences. It may identify technically feasible consequences/options only to clarify the decision boundary; it must not select or redefine project architecture.

This preserves the user's rule: **architecture decisions come from the architect; AI coding performs the refactor.**

## D12 — Persistent work state is first-class for long-running refactors

**Decision:** Introduce durable task execution state separate from chat context.

Minimum state:

- approved task spec;
- agent implementation plan;
- discoveries/evidence;
- progress/status;
- verification references;
- unresolved protected decisions.

## D13 — Modernization must be eval-driven and ablation-based

**Decision:** Legacy restrictions are removed only after replacement safeguards/evals exist, then compared against representative tasks.

The migration must be able to answer:

```text
Did removing this restriction improve or preserve task success
without increasing policy/scope/security regressions?
```

If not, refine or retain the necessary constraint.
