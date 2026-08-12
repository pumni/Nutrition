# 01 — Repository Baseline Used for This Plan

## Baseline identity

- Repository: `pumni/Nutrition_backend`
- Branch: `main`
- Baseline commit: `da04e773a214e8f8232db149d1f35f3f0bd61ce1`
- Current behavior release: `foundation-0.6.0`
- Workspace language: Rust edition 2024
- Rust version: 1.97 / toolchain 1.97.1 in project documentation
- Architecture: modular monolith / hexagonal boundaries
- Primary data store: PostgreSQL
- Existing verification: PowerShell + Cargo + Docker Compose

If implementation starts on a different commit, the executor must not silently adapt this plan. It reports:

`BLOCKED_BASELINE_DRIFT: expected da04e773..., found <actual>`

The architect then decides whether the plan can be rebased.

## Current workspace crates

```text
crates/domain
crates/application
crates/adapters
crates/persistence-postgres
crates/api-http
crates/worker
```

Dependency direction is treated as:

```text
domain <- application <- adapters
                      <- persistence-postgres
                      <- api-http / worker
```

## Current key code map

### `crates/domain`

- `src/calculation.rs` — deterministic calculation
- `src/ids.rs` — typed identifiers
- `src/nutrition.rs` — nutrition/evidence domain types
- `src/lib.rs` — exports

Hard boundary: domain does not import Axum, SQLx, Tokio, provider SDKs, clocks or random generators.

### `crates/application`

- `src/analyze.rs` — analysis use case / orchestration
- `src/revise.rs` — clarification/correction revisions
- `src/ports.rs` — application ports
- `src/model.rs` — application request/outcome/snapshot/version models
- `src/normalization.rs` — normalization
- `src/lib.rs` — exports

Current orchestration is parser → food evidence → portion evidence → deterministic calculator → append-only persisted analysis snapshot.

### `crates/adapters`

- `src/fixture.rs` — local/CI deterministic parser and test doubles
- `src/hosted_parser.rs` — bounded hosted parser anti-corruption adapter
- `src/lib.rs`

Hosted provider output is untrusted and must be schema + semantically validated.

### `crates/persistence-postgres`

Observed source files include:
- `analysis_repository.rs`
- `catalog_repository.rs`
- `portion_repository.rs`
- `ops_repository.rs`
- `parser_telemetry.rs`
- `seed.rs`
- `lib.rs`

Persistence owns transactional analysis writes, catalog/portion evidence, worker operations and parser telemetry.

### `crates/api-http`

- `src/main.rs` — Axum process and composition root

### `crates/worker`

- `src/main.rs` — worker process foundation

## Current non-code governance

Important existing sources:
- `README.md`
- `docs/FOUNDATION_DECISIONS.md`
- `docs/HOSTED_PARSER.md`
- `docs/RISK_REGISTER.md`
- `docs/SECURITY_AND_OPERATIONS.md`
- `docs/SOURCE_REGISTER.md`
- `nutrition_backend_blueprint_v1.0/00_README.md`
- `nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
- `nutrition_backend_blueprint_v1.0/13_IMPLEMENTATION_CHECKLIST.md`
- other blueprint domain/API/testing/security/release documents

## Existing verification contract

Provider-independent verification already checks:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- JSON artifacts parse
- prohibited sensitive logging patterns
- Docker Compose configuration

PostgreSQL integration has a separate `scripts/verify-postgres.ps1`.

## Existing product invariants relevant to AI coding

The ACL must preserve, and make highly visible, at least these invariants:

1. Nutrition numbers come from evidence + deterministic calculation, not LLM invention.
2. Hosted LLM is constrained to structured language extraction.
3. Unknown food is not force-matched.
4. Portion conversion is food/context/source specific, not a global household conversion.
5. Published data is immutable/versioned.
6. Analysis corrections append revisions; they do not overwrite history.
7. Replay is pinned to a behavior version vector.
8. PostgreSQL is source of truth; infrastructure is not added without evidence + ADR.
9. Raw meal text is sensitive and is not logged in telemetry.
10. Hosted parser does not silently fall back to fixture behavior.
11. Applied database migrations are forward-only and must not be edited.
12. New infrastructure or invariant changes require an architecture decision, not an executor improvisation.
