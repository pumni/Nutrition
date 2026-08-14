# 07 — Completion Criteria

The modernization is complete only when all criteria below are satisfied. Partial completion must remain explicitly labeled as transitional.

## 1. Authority model

- Human/architect remains authoritative for protected project decisions.
- Coding-agent documentation no longer describes the agent as a purely mechanical `implementation_only` executor.
- The modern role is policy-bounded implementation autonomy.
- Protected decisions fail closed when absent.
- Agent reports do not silently approve architecture/product/security/privacy/API/database/release decisions.

## 2. Task contract

- Human-authored modern Task Spec exists and is validated.
- Objective and acceptance criteria are first-class.
- Risk level and protected boundaries are first-class.
- Scope envelope is first-class.
- Mandatory gate IDs are first-class.
- `implementation_sequence` is not mandatory architect input.
- exact predeclared `create_files` / `modify_files` / `delete_files` are not the default task model.

## 3. Agent-owned execution state

- Agent plan is a separate artifact from architect task authority.
- Agent can revise its own plan after repository/test evidence.
- Durable progress/evidence state exists for long-running work.
- Stale baseline/work-state detection exists.
- Durable state does not contain prohibited sensitive content.

## 4. Context architecture

- `AGENTS.md` remains a short navigation/authority entrypoint.
- Stable human policy is separated from generated/discoverable implementation facts.
- Context presets/modules support progressive disclosure.
- Agent can combine relevant modules for cross-cutting tasks.
- Initial context does not prohibit inspecting additional relevant repository evidence.
- Context budgets remain enforced.

## 5. Provenance and freshness

- Every provenance path resolves to an actual repository path.
- Source dependency declarations and source-lock coverage agree.
- Code files used as sources of compiled context are freshness-tracked.
- Missing declared sources fail verification.
- Changing a declared source without valid context refresh is detectable.
- Source-lock membership is not a hard-coded historical list unrelated to the source registry.

## 6. Verification truth

- Mandatory executable requirements have canonical gate IDs.
- Natural-language context does not claim nonexistent mandatory gates.
- Trusted runner remains the owner of canonical command execution.
- Task/report artifacts do not redefine arbitrary command truth.
- Existing foundation, PostgreSQL, schema, privacy/logging, replay, and CI trust-boundary checks remain intact unless explicitly replaced with stronger approved checks.

## 7. Scope verification

- Default modern tasks verify `actual changes ⊆ approved scope envelope`.
- Protected paths/contracts remain fail-closed.
- Exact-file mode remains available only where risk policy requires it.
- Legitimate additional implementation files inside approved scope do not cause unnecessary blocking.
- Unrelated cleanup remains detectable/rejected by evals/review.

## 8. Behavioral evals

At least 15 representative behavioral eval cases exist and cover:

- context discovery;
- root-cause quality;
- scope quality;
- invariant preservation;
- protected-decision detection;
- verification recovery;
- escalation quality;
- context efficiency;
- plan adaptability;
- diff self-review.

Behavioral evals score observable behavior/results and do not depend on private chain-of-thought.

## 9. Ablation evidence

Before legacy cognitive restrictions are finally removed, there is recorded comparison evidence showing that the modern mode:

- preserves protected-policy correctness;
- preserves or improves task completion;
- does not introduce material security/privacy/scope regressions;
- reduces unnecessary blocking or architect micromanagement.

If a removed restriction caused regression, the final architecture contains the smallest demonstrated corrective constraint rather than blindly restoring the whole legacy system.

## 10. Legacy removal

The following are removed/deprecated from the default path after modern replacements are proven:

- mandatory architect-authored implementation sequence;
- default exact architect-predicted changed-file sets;
- exclusive architect-selected single context profile;
- mechanical executor identity;
- prohibition on relevant context expansion;
- obsolete v1-only contract fields after compatibility support ends.

## 11. Non-regression guarantees

The modernization must not weaken the repository's existing core truths:

- deterministic nutrition calculation;
- evidence-first nutrition semantics;
- PostgreSQL primary-source-of-truth architecture;
- append-only/completed revision immutability;
- replay/version reproducibility;
- LLM parser-only trust boundary;
- no LLM-generated nutrition facts/gram estimates/canonical IDs;
- sensitive logging/privacy restrictions;
- forward-only migration policy;
- human-controlled canonical publication;
- trusted CI/control-plane boundaries.

## 12. Final repository mental model

A new coding agent should be able to understand the system as:

```text
Human owns project decisions
        ↓
Repository policies define non-negotiable truths
        ↓
Task Spec defines approved outcome and boundaries
        ↓
Agent investigates current repository reality
        ↓
Agent chooses implementation plan inside policy
        ↓
Canonical verification measures outcome/invariants
        ↓
Agent self-corrects implementation failures
        ↓
Human is involved only for protected decisions
```

The migration is not complete if the system still requires the architect to precompute the coding agent's exact implementation trajectory for normal internal engineering work.
