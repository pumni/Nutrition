# P01 — Scaffold ACL Tree

## Context profile

`agent-context-maintenance` once profile exists; for bootstrap use this packet + plan only.

## Allowed creates

- `.agent/README.md`
- `.agent/manifest.json`
- directory placeholders by creating final files specified in this packet only

## Forbidden

- `AGENTS.md`
- `crates/**`
- `migrations/**`
- `Cargo.toml`
- `Cargo.lock`
- `scripts/verify.ps1`

## Exact decisions

- schema version: `1.0.0`
- context release: `agent-context-1.0.0`
- project behavior release: `foundation-0.6.0`
- canonical formats: Markdown + JSON
- no runtime integration

## Steps

1. Create `.agent/` directory tree from `02_TARGET_AGENT_CONTEXT_ARCHITECTURE.md`.
2. Create `.agent/README.md` explaining directories, canonical truth, profile/task lifecycle.
3. Create `.agent/manifest.json` with budgets and canonical index paths exactly defined in `06_MACHINE_READABLE_CONTRACTS.md`.
4. Do not create root `AGENTS.md`.
5. Do not integrate verifier yet.

## Acceptance

- tree exists;
- manifest JSON parses;
- no forbidden files changed.
