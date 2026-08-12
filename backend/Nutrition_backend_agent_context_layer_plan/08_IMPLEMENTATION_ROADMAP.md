# 08 — Implementation Roadmap

The roadmap is fixed. Each phase is executed as a separate task packet.

## Phase P00 — Preflight / no changes

Purpose:
- prove the local checkout matches the analyzed baseline;
- prove current repository verification is green;
- prove there is no pre-existing ACL that would be overwritten.

Required observations:
- `git rev-parse HEAD`
- `git status --short`
- existence check for `AGENTS.md`
- existence check for `.agent`
- run current `.\scripts\verify.ps1`

Stop if:
- commit differs from plan baseline;
- worktree contains conflicting changes;
- root `AGENTS.md` or `.agent/` already exists unexpectedly;
- foundation verification is red.

No files changed.

## Phase P01 — Scaffold canonical ACL tree

Create directories and:
- `.agent/README.md`
- `.agent/manifest.json`
- placeholder/initial canonical files required by manifest
- no root `AGENTS.md` yet
- no existing verification integration yet

Why root entrypoint comes later:
- the repository must not tell coding agents to use an incomplete layer.

Gate:
- all JSON parses;
- required tree exists.

## Phase P02 — Authority and invariants

Create:
- authority files;
- invariant files.

Content must be sourced from existing project governance and this plan, not invented.

Gate:
- file size budgets;
- source references present;
- no product/runtime files changed.

## Phase P03 — Context packs and maps

Create:
- module context packs;
- crate map;
- change impact map;
- verification map;
- source register;
- context profiles.

Gate:
- every referenced path exists;
- profile integrity;
- path rules do not conflict.

## Phase P04 — Contracts and templates

Create:
- task packet schema;
- verification report schema;
- implementation report schema;
- example task packet;
- example reports.

Critical rule:
`decision_points.maxItems = 0` in JSON Schema.

Gate:
- JSON parse;
- examples satisfy manually enforced structural validator.

## Phase P05 — Deterministic verifier + evals

Create:
- `scripts/verify-agent-context.ps1`
- `.agent/evals/README.md`
- `.agent/evals/context-layer-cases.json`
- `.agent/state/source-lock.json`

Implement:
- default;
- `-TaskPacket`;
- `-SelfTest`.

Compute source hashes exactly as packet lists.

Gate:
- all 12 required self-tests;
- default verifier passes.

## Phase P06 — Activate root entrypoint

Create `AGENTS.md`.

It must:
- be <= 4096 bytes;
- require a task packet before writes;
- point to manifest;
- state executor-only authority;
- require named context profile;
- define block behavior;
- require verification report.

Gate:
- default verifier passes;
- root size budget passes.

## Phase P07 — Integrate with repository verification

Modify only:
- `scripts/verify.ps1`;
- `README.md` (short ACL developer section).

Exact verify change:
- invoke agent-context verifier before Cargo checks.

README change:
- explain that coding agents use `AGENTS.md`;
- state ACL does not alter runtime behavior;
- show three verification commands;
- point human architects to task packet template.

Gate:
- self-test;
- ACL default verification;
- entire `scripts/verify.ps1`.

## Phase P08 — Final system eval and report

No architecture changes.

Run:
- self-tests;
- default ACL verification;
- foundation verification;
- task-mode verification using valid example packet;
- negative task-mode fixture proving forbidden path failure.

Inspect diff:
- no `crates/**`;
- no migrations;
- no Cargo files;
- no deploy/config behavior changes.

Produce final implementation report.

## Phase completion rule

A phase is complete only when:
- its acceptance criteria pass;
- verification evidence is captured;
- no scope deviation exists.

An executor cannot mark a phase "mostly complete" and start the next one.
