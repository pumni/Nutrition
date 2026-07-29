# Nutrition backend

Evidence-first nutrition analysis backend based on
[`nutrition_backend_blueprint_v1.0`](nutrition_backend_blueprint_v1.0/00_README.md).

Current behavior release: `foundation-0.2.0`.

## Implemented foundation slice

```text
explicit grams
→ deterministic fixture parser
→ exact PostgreSQL catalog lookup
→ direct composition profile
→ pure decimal calculator
→ transactional PostgreSQL analysis snapshot
→ hash-verified read/replay
```

This is deliberately narrower than the walking skeleton. It proves domain boundaries,
calculation semantics, behavior versioning, unknown-food rejection, PostgreSQL transaction
boundaries, immutable persistence, and create/read replay.

## Prerequisites

- Rust `1.97.1`.
- Docker with Compose for PostgreSQL integration work.

## Verify

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Or run the provider-independent verification contract:

```powershell
.\scripts\verify.ps1
```

## Start PostgreSQL and prepare local fixtures

```powershell
docker compose -f deploy/compose.yaml up -d postgres

$env:DATABASE_URL = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition"
$env:RUN_MIGRATIONS = "true"
$env:RUN_FOUNDATION_SEED = "true"
cargo run -p worker
```

The foundation seed is explicitly test-only and cannot be treated as production nutrition
evidence.

## Run the foundation API

```powershell
$env:APP_BIND_ADDR = "127.0.0.1:8080"
$env:DATABASE_URL = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition"
$env:RUST_LOG = "info"
cargo run -p api-http
```

Health and readiness:

```http
GET /health/live
GET /health/ready
```

Foundation analysis request:

```http
POST /v1/nutrition/analyses
Content-Type: application/json

{
  "text": "100 g trứng gà luộc, 150 g cơm trắng",
  "locale": "vi-VN",
  "mode": "balanced"
}
```

Only two fixture foods are available and the parser accepts explicit grams. Unknown foods return
`analysis_insufficient`; they are never force-matched.

Read the current persisted revision:

```http
GET /v1/nutrition/analyses/{analysis_id}
```

The read path verifies the persisted snapshot SHA-256 before deserialization.

## PostgreSQL

The migrations create seven logical schemas, the minimal walking-skeleton tables, search indexes,
behavior version fields, snapshot persistence, scoped idempotency, and immutability guards.

Apply migrations locally:

```powershell
$env:DATABASE_URL = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition"
$env:RUN_MIGRATIONS = "true"
cargo run -p worker
```

Database migrations are forward-only. Do not edit an applied migration; add a new migration.

Run the full PostgreSQL integration, API smoke, replay, and immutability suite:

```powershell
.\scripts\verify-postgres.ps1
```

## Repository boundaries

- `domain`: IDs, units, evidence semantics, pure deterministic calculator.
- `application`: use cases and ports.
- `adapters`: deterministic fixture parser and in-memory test doubles.
- `persistence-postgres`: migrations, exact catalog lookup, transactional analysis repository,
  snapshot reader, and explicit test-only seed.
- `api-http`: Axum HTTP process.
- `worker`: PostgreSQL-backed worker process foundation.

See [`docs/FOUNDATION_DECISIONS.md`](docs/FOUNDATION_DECISIONS.md) for decisions and deferred scope.
The initial governance artifacts are
[`docs/SOURCE_REGISTER.md`](docs/SOURCE_REGISTER.md),
[`docs/RISK_REGISTER.md`](docs/RISK_REGISTER.md), and the development-only
[`VietnameseMealBench manifest`](fixtures/vietnamese-meal-bench/manifest.json).
