# Nutrition_backend — Agent Context Layer Implementation Pack

**Repository:** `pumni/Nutrition_backend`  
**Analyzed branch:** `main`  
**Analyzed baseline commit:** `da04e773a214e8f8232db149d1f35f3f0bd61ce1`  
**Application behavior release:** `foundation-0.6.0`  
**Planned Agent Context Layer release:** `agent-context-1.0.0`

## Purpose

This pack is the implementation authority for building a repository-level Agent Context Layer (ACL) for AI coding agents.

The ACL is **not a runtime nutrition feature** and **not an LLM memory/RAG service**. It is a deterministic control and context plane stored in the repository so that coding agents:

1. receive only the context relevant to a task;
2. cannot make architecture/product/data/security decisions;
3. are constrained to exact allowed files and changes;
4. must stop on a missing decision instead of inventing one;
5. must prove work with deterministic verification;
6. can be swapped across Claude Code, Codex, Gemini, IDE agents, or future models without rewriting the project's governance.

## Non-negotiable role split

- **Architect / decision authority:** decides architecture, scope, behavior, public contracts, dependencies, migrations, versions, risks, and acceptance criteria.
- **AI coding executor:** writes code/files exactly according to an architect-authored task packet.
- **Verifier:** deterministic scripts/tests decide whether the executor's output satisfies the contract.

AI coding is allowed mechanical implementation freedom only: formatting, import ordering, local variable naming, and syntactic choices that do not change specified behavior. Everything else is a decision and is forbidden unless explicitly stated in the task packet.

## How to use this pack

Read in this order:

1. `00_DECISION_SUMMARY.md`
2. `02_TARGET_AGENT_CONTEXT_ARCHITECTURE.md`
3. `03_AUTHORITY_AND_EXECUTOR_CONTRACT.md`
4. `08_IMPLEMENTATION_ROADMAP.md`
5. `09_FILE_BY_FILE_CHANGESET.md`
6. Execute `task_packets/P00_...` through `P08_...` in order.
7. Give `prompts/AI_CODING_EXECUTOR_PROMPT.md` to the coding agent together with exactly one task packet at a time.

Do **not** hand all task packets to an executor and tell it to "figure out the order". The order is already decided.

## Architectural outcome

The repository will gain:

```text
AGENTS.md                         # thin, vendor-neutral entrypoint
.agent/
  README.md
  manifest.json                  # ACL identity, authority, budgets, global gates
  authority/
    executor-contract.md
    decision-policy.md
    escalation-protocol.md
  invariants/
    product-domain.md
    architecture.md
    data-replay.md
    llm-boundary.md
    security-privacy.md
  contexts/
    foundation.md
    domain.md
    application.md
    parser.md
    persistence.md
    api.md
    worker.md
    data-governance.md
    verification.md
  maps/
    crate-map.json
    change-impact-map.json
    verification-map.json
    source-register.json
  profiles/
    context-profiles.json
  contracts/
    task-packet.schema.json
    verification-report.schema.json
    implementation-report.schema.json
  templates/
    task-packet.example.json
    verification-report.example.json
    implementation-report.example.md
  evals/
    README.md
    context-layer-cases.json
  state/
    source-lock.json
scripts/
  verify-agent-context.ps1
```

Existing `scripts/verify.ps1` will call `scripts/verify-agent-context.ps1` as a cheap, deterministic first gate.

## Explicitly out of scope

The implementation MUST NOT:

- add a new Rust crate;
- change any file under `crates/**`;
- add or remove Cargo dependencies;
- change any migration or schema;
- change API behavior;
- change parser prompt/schema/provider behavior;
- change nutrition calculation behavior;
- change application behavior versions;
- introduce a vector database, embedding index, Redis, Kafka, MCP server, agent framework, or vendor SDK;
- add model-specific governance as the canonical source of truth;
- create autonomous planning logic that lets the executor decide architecture.

The ACL is intentionally small, inspectable, deleteable, and model-neutral.
