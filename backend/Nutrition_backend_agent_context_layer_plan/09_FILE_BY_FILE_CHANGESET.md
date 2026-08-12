# 09 — File-by-File Changeset Specification

This file removes file-design freedom from the coding executor.

## Root

### `AGENTS.md` — CREATE in P06

Purpose: thin bootloader.

Must contain only:
- role declaration;
- mandatory task packet rule;
- manifest path;
- context-profile loading protocol;
- no-autonomous-decision rule;
- block-state rule;
- verification/report requirement.

Must not:
- duplicate detailed project architecture;
- contain vendor-specific commands;
- exceed 4096 bytes.

### `README.md` — MODIFY in P07

Append a concise section titled `## AI coding context layer`.

Must state:
- canonical entrypoint is `AGENTS.md`;
- architect writes task packet;
- executor does not make design decisions;
- commands:
  - `.\scripts\verify-agent-context.ps1 -SelfTest`
  - `.\scripts\verify-agent-context.ps1`
  - `.\scripts\verify.ps1`
- ACL is repository tooling and does not participate in nutrition runtime.

Do not alter existing foundation behavior documentation.

## `.agent/README.md`

Explain directory semantics, source-of-truth policy, profile loading, task packet lifecycle and maintenance rules.

No project decisions unique to this file; link to sources.

## `.agent/manifest.json`

Exact release identity:
- schema `1.0.0`;
- context release `agent-context-1.0.0`;
- project behavior release `foundation-0.6.0`.

Declare byte budgets and canonical index paths.

## Authority files

### `authority/executor-contract.md`
Operational version of `03_AUTHORITY_AND_EXECUTOR_CONTRACT.md`.

### `authority/decision-policy.md`
Classify:
- architect-only decisions;
- mechanical implementation choices;
- required impact declarations.

### `authority/escalation-protocol.md`
Exact block codes and report fields.

## Invariant files

### `invariants/product-domain.md`
Include:
- evidence-first numbers;
- no force-match unknown food;
- contextual portion evidence;
- deterministic calculator;
- estimate/uncertainty semantics.

### `invariants/architecture.md`
Include:
- modular monolith;
- crate dependency direction;
- PostgreSQL source of truth;
- no premature infrastructure;
- domain purity;
- forward-only migrations.

### `invariants/data-replay.md`
Include:
- published immutability;
- append-only analysis revisions;
- hash-verified snapshots;
- behavior version vector;
- release pinning/idempotency.

### `invariants/llm-boundary.md`
Include:
- hosted LLM language extraction only;
- strict schema;
- output untrusted;
- source grounding;
- no nutrient/ID/gram invention;
- bounded retry;
- fail closed;
- no fixture fallback.

### `invariants/security-privacy.md`
Include:
- raw meal text sensitive;
- no raw text/auth/database URL logging;
- minimum hosted request envelope;
- ownership/auth boundaries;
- hosted provider approval gate.

## Context files

Each starts with:
- `Context release: agent-context-1.0.0`
- `Derived from:` source paths
- `Use when:` profile(s)
- `Do not infer:` decisions beyond packet

### `contexts/foundation.md`
High-level vertical slice and behavior release.

### `contexts/domain.md`
Domain types, calculator responsibility, evidence semantics, forbidden dependencies.

### `contexts/application.md`
Ports, `MealAnalysisService`, analysis flow, clarification/correction, version vector.

### `contexts/parser.md`
Fixture vs hosted, adapter contract, schema/semantic validation, telemetry privacy.

### `contexts/persistence.md`
Repository responsibilities, transaction/immutability/idempotency/replay/migration rules.

### `contexts/api.md`
Axum composition root, auth/ownership/idempotency/request-limit/security boundaries.

### `contexts/worker.md`
Worker modes, leases/retry/dead/outbox/graceful shutdown.

### `contexts/data-governance.md`
Source release staging, curation, test-only fixtures, provenance, publication gate.

### `contexts/verification.md`
Which repository commands prove which classes of change.

## Map files

### `maps/crate-map.json`
For each workspace crate:
- path;
- role;
- key files;
- upstream dependencies;
- forbidden dependencies;
- relevant context profile.

### `maps/change-impact-map.json`
Rules for:
- `crates/domain/**`
- `crates/application/**`
- `crates/adapters/src/hosted_parser.rs`
- `schemas/**`
- `crates/persistence-postgres/**`
- `migrations/**`
- `crates/api-http/**`
- `crates/worker/**`
- `fixtures/**`
- `seeds/**`
- `.agent/**`
- `AGENTS.md`
- `scripts/verify-agent-context.ps1`
- `scripts/verify.ps1`

### `maps/verification-map.json`
Gate definitions:
- `acl-self-test`
- `acl-integrity`
- `cargo-fmt`
- `cargo-clippy`
- `cargo-test`
- `foundation-verify`
- `postgres-verify`
- future benchmark gate marked as external/not universally runnable

### `maps/source-register.json`
Map every context/invariant to canonical source paths.

## Profile file

### `profiles/context-profiles.json`
Implement exact profiles from `05_CONTEXT_PROFILES_SPEC.md`.

## Contracts

### `contracts/task-packet.schema.json`
JSON Schema Draft 2020-12 or the same draft consistently used by examples.

Required:
- `additionalProperties: false` on core packet objects;
- impact object explicit;
- `decision_points` required with `maxItems: 0`.

### `contracts/verification-report.schema.json`
Strict fields for check evidence and scope results.

### `contracts/implementation-report.schema.json`
Strict machine representation of final executor report.

## Templates

### `templates/task-packet.example.json`
A harmless ACL-maintenance example, not a runtime code change.

### `templates/verification-report.example.json`
Pass example.

### `templates/implementation-report.example.md`
Human-readable completion format.

## Evals

### `evals/context-layer-cases.json`
12+ deterministic cases from verification specification.

### `evals/README.md`
Explain expected outcomes and how no case mutates repository state.

## State

### `state/source-lock.json`
Hash list set by P05. Only listed canonical sources.

Initial lock list must include exactly:

1. `Cargo.toml`
2. `docs/FOUNDATION_DECISIONS.md`
3. `docs/HOSTED_PARSER.md`
4. `docs/RISK_REGISTER.md`
5. `docs/SECURITY_AND_OPERATIONS.md`
6. `nutrition_backend_blueprint_v1.0/00_README.md`
7. `nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
8. `nutrition_backend_blueprint_v1.0/13_IMPLEMENTATION_CHECKLIST.md`

Do not add Rust source files to the lock in v1.

## `scripts/verify-agent-context.ps1`

Must implement:
- parameters `[string]$TaskPacket`, `[switch]$SelfTest`;
- strict errors;
- repo-root resolution from `$PSScriptRoot`;
- JSON loader helper;
- required-field helper;
- SHA256 source-lock verification;
- byte-budget verification;
- profile reference verification;
- source-register verification;
- task packet verification;
- wildcard path matching;
- `git diff --name-only` scope verification in task mode;
- isolated self-tests in temporary files/directories;
- nonzero exit/throw on failure;
- clear output prefixes.

Must not:
- make network calls;
- install modules;
- modify repository;
- silently skip git if task mode requested.

## `scripts/verify.ps1` — MODIFY in P07

Add at the beginning, after strict error settings:

```powershell
Write-Output "Validating agent context layer..."
& "$PSScriptRoot\verify-agent-context.ps1"
```

Do not weaken or delete existing checks.
