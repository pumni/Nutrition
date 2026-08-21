# Architecture

## Purpose

`Nutrition_backend` is an evidence-first nutrition analysis backend. Meal language may be parsed
by a bounded LLM adapter, but nutrition resolution and calculation are deterministic, versioned,
and replayable.

## Request flow

```text
meal text
→ parser boundary
→ application orchestration
→ exact food resolution
→ explicit grams or contextual portion evidence
→ versioned composition evidence
→ deterministic decimal calculation
→ transactional revision + immutable snapshot
→ hash-verified read/replay
```

## Workspace dependency direction

```text
domain <- application <- adapters
                      <- persistence-postgres
                      <- api-http / worker (composition edges)
```

`domain` is transport and runtime independent.

## Crates

- `domain` — IDs, evidence semantics, units, and deterministic calculation.
- `application` — use cases, workflow models, ports, and clarification/correction orchestration.
- `adapters` — fixture parser, hosted parser boundary, and external/provider mappings.
- `persistence-postgres` — catalog/evidence lookup, revisions/snapshots, privacy/data operations,
  and worker persistence.
- `api-http` — HTTP composition, authentication, routes, and request/response mapping.
- `worker` — migrations/seed gates and bounded background work modes.

## Deep dives

- Parser boundary: [docs/architecture/parser.md](docs/architecture/parser.md)
- Foundation behavior: [docs/architecture/foundation.md](docs/architecture/foundation.md)
- API contract: [docs/product/api-v1.md](docs/product/api-v1.md)
- Evidence model: [docs/evidence/portions.md](docs/evidence/portions.md)
- Operations/configuration: [docs/operations/configuration.md](docs/operations/configuration.md)
- Security/privacy: [docs/operations/security.md](docs/operations/security.md)
- Documentation router: [docs/index.md](docs/index.md)

## Verification

```powershell
cargo xtask check
```

Use `cargo xtask postgres`, `cargo xtask fdc`, `cargo xtask containers`, and `cargo xtask benchmark`
for specialized verification.
