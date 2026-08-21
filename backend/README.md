# Nutrition backend

Evidence-first Rust backend for Vietnamese meal analysis. Language parsing may use a bounded
adapter, but food identity, portion mass, composition, calories, and persisted evidence come from
deterministic, versioned system evidence.

Current behavior release: [foundation-0.6.0](docs/releases/foundation-0.6.0.md).

## Foundation slice

```text
meal text
  → bounded parser boundary
  → exact catalog lookup and reviewed portion evidence
  → deterministic decimal calculation with bounds
  → transactional PostgreSQL snapshot and immutable revisions
```

Unknown or unsupported evidence fails closed. Clarification and correction are explicit revision
flows; replay is bound to recorded behavior and evidence versions.

## Prerequisites

- Rust `1.97.1`
- Docker with Compose for PostgreSQL and container verification

## Verify

```powershell
cargo xtask check
cargo xtask postgres       # PostgreSQL, HTTP, worker, and immutability integration
cargo xtask containers     # production-container readiness and non-root checks
```

Use `cargo xtask fdc`, `cargo xtask benchmark`, or `cargo xtask all` when the changed boundary
requires them. The normal product gate is `cargo xtask check`.

## Run locally

Start PostgreSQL and the development-only foundation fixtures:

```powershell
docker compose -f deploy/compose.yaml up -d postgres

$env:APP_ENV = "local"
$env:DATABASE_URL = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition"
$env:RUN_MIGRATIONS = "true"
$env:RUN_FOUNDATION_SEED = "true"
cargo run -p worker
```

Run the API in another shell:

```powershell
$env:APP_ENV = "local"
$env:APP_BIND_ADDR = "127.0.0.1:8080"
$env:DATABASE_URL = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition"
$env:AUTH_MODE = "development"
$env:PARSER_MODE = "fixture"
cargo run -p api-http
```

Development auth, fixture parsing, and foundation seeds are accepted only in local/CI behavior.
Staging and production fail closed unless their explicit provider and deployment configuration is
approved. See [operations/configuration.md](docs/operations/configuration.md).

## API surface

The current HTTP contract is documented in [product/api-v1.md](docs/product/api-v1.md) and the
canonical OpenAPI document at [openapi/nutrition-api-v1.json](openapi/nutrition-api-v1.json). The primary
analysis routes are:

```text
GET  /health/live
GET  /health/ready
POST /v1/nutrition/analyses
GET  /v1/nutrition/analyses/{analysis_id}
GET  /v1/nutrition/analyses/{analysis_id}/revisions/{revision_number}
POST /v1/nutrition/analyses/{analysis_id}/clarifications
POST /v1/nutrition/analyses/{analysis_id}/corrections
```

## Repository map

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — crate ownership and request flow
- [`docs/index.md`](docs/index.md) — current documentation router
- `crates/domain` — IDs, units, evidence semantics, and deterministic calculation
- `crates/application` — use cases and ports
- `crates/adapters` — fixture parser and bounded hosted-parser adapter
- `crates/persistence-postgres` — migrations, catalog, snapshots, revisions, and privacy paths
- `crates/api-http` — Axum HTTP process
- `crates/worker` — PostgreSQL-backed worker process
- `crates/xtask` — deterministic verification commands

## Release boundary

Production traffic, provider selection, catalog activation, benchmark publication, and release
publication remain human-controlled gates. Foundation fixtures and `VietnameseMealBench` remain
development-only; this repository does not claim production readiness.

Start coding-agent work with [`AGENTS.md`](AGENTS.md). Vendor-specific adapters under `.claude/`
are optional pointers; canonical truth remains in source, tests, docs, and `cargo xtask`.
