# 02 — Target Agent Context Architecture

## 1. Design goal

The ACL must maximize **task success per unit of context**, not maximize tokens sent to the model.

The architecture therefore has five layers:

```text
Layer 0 — AGENTS.md
  thin bootloader; universal protocol only

Layer 1 — manifest + authority
  what the ACL is, who decides, what is forbidden

Layer 2 — invariants
  compact non-negotiable product/architecture/security truths

Layer 3 — task-selected context profile
  only domain/application/parser/etc. context needed for the task

Layer 4 — task packet
  exact implementation scope, paths, sequence, acceptance and verification

Layer 5 — deterministic verifier
  checks context integrity + checks executor stayed inside contract
```

## 2. Target tree

```text
AGENTS.md
.agent/
  README.md
  manifest.json

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

## 3. Why no new crate

A coding-agent context system is repository governance, not production domain behavior.

Creating a Rust crate would:
- increase build graph and maintenance;
- risk coupling product runtime with coding tool behavior;
- imply the ACL is a product service;
- make deletion harder;
- violate the desired "minimum harness" principle.

Therefore v1 stays outside Cargo.

## 4. Why JSON instead of YAML

The repository currently verifies JSON with PowerShell `ConvertFrom-Json`.

JSON lets the project:
- validate syntax without adding tooling;
- version schemas;
- make machine contracts explicit;
- keep CI deterministic.

YAML offers no required capability for ACL v1.

## 5. Source-of-truth model

### Canonical truth remains where it already is

Examples:
- architecture decisions → blueprint ADRs / foundation decisions;
- hosted parser boundary → `docs/HOSTED_PARSER.md`;
- risk controls → risk/security docs;
- actual structure → repository files;
- task-specific decision → architect-authored task packet.

### `.agent` content is compiled context

Every `.agent/contexts/*.md` and `.agent/invariants/*.md` file:
- is concise;
- links to source paths/headings;
- does not silently redefine policy;
- has corresponding source entries in `maps/source-register.json`.

## 6. Context budget

The following hard limits are selected for v1:

- `AGENTS.md`: <= 4 KiB
- `.agent/README.md`: <= 8 KiB
- each authority file: <= 12 KiB
- each invariant file: <= 12 KiB
- each context pack: <= 16 KiB
- total files loaded by a normal context profile: target <= 48 KiB excluding the task packet
- task packet: target <= 24 KiB

The verifier fails files above hard limits. The point is to make prompt/context growth measurable.

## 7. Context profiles

Profiles are explicit. The executor never selects one.

Required profiles:

1. `core`
2. `domain-calculation`
3. `application-analysis`
4. `parser-hosted`
5. `persistence-postgres`
6. `api-http`
7. `worker-ops`
8. `data-governance`
9. `agent-context-maintenance`

Each profile defines:
- `required_context_files`;
- `allowed_path_patterns`;
- `mandatory_verification_gates`;
- `sensitive_invariants`;
- `forbidden_decisions`.

## 8. Task packets are the unit of delegation

A task packet is an executable contract, not a prose request.

It must specify:
- objective;
- exact profile;
- files allowed;
- files forbidden;
- files to create;
- files to modify;
- exact implementation sequence;
- externally visible behavior;
- version impact;
- dependency impact;
- database impact;
- acceptance criteria;
- verification commands;
- escalation conditions.

`decision_points` is required and MUST be an empty array. If a design choice exists, the architect has not finished the packet.

## 9. Deterministic change control

When a task packet is provided, `verify-agent-context.ps1` can optionally inspect `git diff --name-only` and fail if:
- a changed path is not allowed;
- a forbidden path changed;
- a task says "no dependency change" but Cargo manifests changed;
- a task says "no database impact" but migrations changed;
- task declares no behavior impact but behavior-version source changed.

## 10. Context drift control

`state/source-lock.json` contains SHA-256 hashes only for selected governance sources summarized by ACL.

When those sources change, verification fails until the architect-approved ACL summaries and lock are refreshed.

The lock must not include all Rust files. Doing so would make ordinary coding work constantly invalidate context.

## 11. Vendor neutrality

No canonical `.agent` file may assume:
- Claude-specific commands;
- Codex-specific instructions;
- Gemini-specific features;
- Cursor/Windsurf internals;
- proprietary memory format.

Vendor wrappers may later point to `AGENTS.md`, but they are not authoritative.

## 12. Deletion criterion

The ACL is considered well-designed if it can be removed by deleting:
- `AGENTS.md`;
- `.agent/`;
- `scripts/verify-agent-context.ps1`;
- the single call added to `scripts/verify.ps1`;
- the short README documentation section.

No product code or migration must depend on it.
