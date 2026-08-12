# 00 — Locked Decision Summary

These decisions are already made. The coding executor must not reopen them.

## D-001 — ACL is repository-level, not product runtime

The phrase **Agent Context Layer** in this plan means a repository-local context/control plane for AI coding. It does not participate in user nutrition requests.

**Reason:** the current product deliberately constrains hosted LLM use to language extraction while food evidence, portions, composition and calculation remain deterministic. A coding-agent context system must not leak into that product boundary.

## D-002 — No new runtime dependency

ACL v1 is files + PowerShell verification only.

No Rust crate, Python dependency, Node package, container, database table, network service or external agent framework will be added.

## D-003 — Tool/model-neutral canonical format

Canonical files are Markdown and JSON.

- Markdown: concise human/model context.
- JSON: machine-readable manifests, profiles, maps, contracts and eval cases.
- PowerShell: deterministic verification because the repository already uses PowerShell verification.

No YAML is used because parsing YAML would add a dependency or rely on environment-specific modules.

## D-004 — Root instruction is intentionally small

`AGENTS.md` is an entrypoint, not a knowledge dump.

It must point the executor to:
- `.agent/manifest.json`;
- authority rules;
- a task packet;
- a named context profile;
- verification commands.

It must remain under the configured byte budget.

## D-005 — Context is loaded by explicit profile

The executor is never told to read every blueprint document.

Each architect-authored task packet names one `context_profile`. That profile deterministically lists the context files required for the task.

If `context_profile` is missing or unknown, the executor must stop with:

`BLOCKED_DECISION: CONTEXT_PROFILE_REQUIRED`

## D-006 — Executor may not choose architecture

The executor may not independently:
- create public APIs;
- choose abstractions;
- add dependencies;
- add infrastructure;
- create migrations;
- choose persistence shapes;
- alter domain semantics;
- change security/privacy policy;
- choose provider/model/prompt behavior;
- change behavior versions;
- widen scope;
- refactor unrelated code.

The task packet must contain those decisions before implementation starts.

## D-007 — Source-linked summaries, not duplicate truth

Existing project documents and code remain authoritative.

`.agent/` contains compact, source-linked summaries and maps. `source-register.json` records the authoritative source for every context artifact. `source-lock.json` detects drift in the small set of governance sources that the summaries compile.

## D-008 — Deterministic gates before model judgement

The primary acceptance mechanism is deterministic:
- path allowlist;
- forbidden path detection;
- required file existence;
- JSON parse/shape checks;
- source-lock hashes;
- context size budgets;
- profile integrity;
- task packet structural rules;
- repository verification.

LLM review may be used later as an additional reviewer, never as the only gate.

## D-009 — Existing project behavior remains unchanged

ACL v1 is a governance/tooling release only.

Expected behavior impact:
- domain: none;
- parser: none;
- persistence: none;
- API: none;
- worker: none;
- database: none;
- application behavior version: unchanged (`foundation-0.6.0`).

## D-010 — Implementation is phased and atomic

AI coding executes one packet at a time: `P00` → `P08`.

A later packet may not be started until the previous packet's required gates pass.
