# Nutrition Backend — Coding Agent Guide

Evidence-first Rust backend for Vietnamese meal analysis. Language parsing may use a bounded
LLM adapter, but nutrition identity, portion mass, composition values, calories, and persisted
evidence come from deterministic, versioned system evidence.

## Start here

- Architecture map: [ARCHITECTURE.md](ARCHITECTURE.md)
- Documentation index: [docs/index.md](docs/index.md)
- Runtime behavior: [docs/architecture/foundation.md](docs/architecture/foundation.md)
- Parser boundary: [docs/architecture/parser.md](docs/architecture/parser.md)
- Operations/configuration: [docs/operations/configuration.md](docs/operations/configuration.md)

## Stable invariants

- Never invent nutrition facts, canonical food IDs, gram weights, or calories from LLM output.
- Unknown or unsupported evidence fails closed; do not force-match food or hide portion assumptions.
- Published catalog evidence and completed analysis revisions are immutable and versioned.
- Replay must not depend on unrecorded current configuration.
- Existing recorded migrations are immutable; database changes use new forward migrations.
- Never log raw meal text, authorization material, database credentials, provider secrets, or raw provider responses.
- Development fixture parsers and seeds remain isolated to local/CI behavior.
- Production catalog activation, provider enablement/deployment, and release publication are human-controlled effects.

## How to work

For an unfamiliar subsystem, inspect its source, direct tests, and the relevant page from
[docs/index.md](docs/index.md) before editing. Search the repository instead of loading unrelated
documentation up front.

Make normal implementation decisions autonomously when behavior is already defined. Ask for a
human decision only when the task would choose a genuinely undefined product, domain, privacy, or
release semantic—not merely because an important file must change.

## Context boundaries

The filesystem and [docs/index.md](docs/index.md) are the context router; there is no generated
context manifest to maintain. Claude-specific rules and Skills are optional adapters under
`.claude/` and must stay thin. Do not add nested agent instructions, hooks, MCP servers, or a
replacement protocol without evidence from repeated task failures.

## Verification

The normal product verification entry point is:

```text
cargo xtask check
```

Use `cargo xtask postgres`, `cargo xtask fdc`, `cargo xtask containers`, `cargo xtask benchmark`,
or `cargo xtask all` when the changed boundary requires them. Run targeted tests during
development, inspect the actual diff before finishing, and report the commands that passed.

## Done

Preserve runtime behavior unless the task explicitly changes it, keep sensitive data out of logs
and commits, leave no unrelated churn, and finish with a verified, reviewable diff. Prefer the
smallest change that leaves the repository easier to navigate.
