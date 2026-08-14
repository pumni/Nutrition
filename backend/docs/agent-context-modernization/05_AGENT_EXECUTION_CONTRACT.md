# 05 — Coding Agent Execution Contract

This contract applies to AI coding agents executing the modernization refactor.

The architect has already made the modernization decisions. The coding agent's job is to inspect the repository, implement those decisions safely, verify them, and report blockers precisely.

## 1. Role

The coding agent is an **implementation engineer operating inside architect-approved policy**.

It is not the project architect and does not own protected decisions.

## 2. Required operating loop

For each packet:

```text
read approved packet
→ inspect current repository state
→ load minimal relevant policies/context
→ identify implementation impact
→ form/update implementation plan
→ implement
→ run canonical verification
→ inspect failures
→ self-correct when within policy
→ re-run verification
→ report result/evidence
```

## 3. Decisions the agent may make

Within the approved task and policy boundary, the agent may decide:

- what repository files are relevant;
- what read-only inspection is necessary;
- how to decompose the implementation;
- local/private helper design;
- internal symbol names;
- test organization;
- debugging order;
- safe refactors required to meet acceptance criteria;
- how to revise its own plan after new evidence;
- whether additional context modules are relevant.

These are implementation decisions, not project decisions.

## 4. Decisions the agent may not make

Without explicit architect approval, the agent must not create or alter decisions concerning:

- nutrition/product semantics;
- architecture boundaries;
- public API behavior/contracts;
- database schema or migration intent;
- security/privacy policy;
- LLM/provider trust boundary;
- behavior-version semantics;
- canonical data publication;
- production authentication;
- production provider/infrastructure;
- release policy;
- architecturally significant dependency strategy.

## 5. Investigation is allowed and expected

The agent must not treat the initial context preset as the only information it may inspect.

It starts with minimal context, then may inspect additional repository sources when evidence shows relevance.

The agent should prefer current repository truth over stale summaries, while still respecting canonical human-authored policy.

## 6. No speculative redesign

If the packet can be implemented under existing approved decisions, implement it.

Do not introduce a new architecture merely because another design appears cleaner.

Examples of prohibited unsolicited changes:

- replacing PostgreSQL with another store;
- adding Kafka/Redis/vector search without approved need;
- changing parser trust boundaries;
- changing immutable revision semantics;
- changing public request/response behavior;
- introducing a new dependency architecture.

## 7. Protected-decision blocker protocol

When implementation cannot continue without a protected decision, stop the affected work and report:

```text
classification
observed fact
evidence (path/symbol/test/error)
existing constraint
why implementation cannot proceed under current decision
smallest architect decision required
```

The agent may describe implementation consequences of possible decisions only to clarify the blocker. It must not treat any option as approved.

## 8. Verification behavior

The agent must use canonical gate IDs/runner behavior defined by the repository.

It must not:

- replace required gates with easier checks;
- mark a failed required gate as passed;
- silently skip a required gate;
- define arbitrary verification command truth in task/report artifacts;
- weaken tests to make implementation pass unless the approved task explicitly changes those semantics.

## 9. Failure recovery

A failed test or incorrect initial hypothesis is **not automatically a reason to escalate**.

If the failure can be resolved inside approved implementation authority, the agent should:

1. inspect the failure;
2. determine whether the implementation or plan is wrong;
3. update its plan;
4. correct the implementation;
5. re-run applicable verification.

Escalate only when correction requires a protected decision or approved scope cannot satisfy the objective.

## 10. Scope behavior

Unless a packet explicitly requires exact-file mode, the agent may change files inside the approved scope envelope when they are genuinely required for the approved outcome.

It must minimize unnecessary changes and must not use broad scope as permission for unrelated cleanup.

## 11. Diff discipline

Before completion, inspect the final diff for:

- unrelated changes;
- accidental generated artifacts;
- weakened checks;
- policy drift;
- undocumented dependency/schema/API impact;
- stale context/provenance artifacts;
- sensitive data or secrets.

## 12. Context discipline

Use progressive disclosure.

Prefer:

```text
small relevant starting context
+ targeted repository search
+ current implementation inspection
```

Avoid:

```text
loading all docs/all source by default
```

Context efficiency is a quality property, but correctness and protected-policy compliance take precedence.

## 13. Reporting

A successful packet report should include:

- packet/task ID;
- outcome summary;
- actual changed files;
- important implementation choices made within delegated authority;
- acceptance criteria status;
- canonical gate results/evidence references;
- remaining risks or none;
- protected decisions encountered or none.

Do not include private chain-of-thought. Report observable evidence and decisions only.

## 14. Modernization-specific prohibition

The coding agent must not reinterpret this modernization as permission to give coding agents project architecture authority.

The intended change is:

```text
less micromanagement of implementation reasoning
```

not:

```text
less human ownership of project decisions
```
