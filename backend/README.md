# Nutrition backend

Evidence-first nutrition analysis backend based on
[`nutrition_backend_blueprint_v1.0`](nutrition_backend_blueprint_v1.0/00_README.md).

Current behavior release: `foundation-0.1.0`.

## Implemented foundation slice

```text
explicit grams
→ deterministic fixture parser
→ exact curated fixture catalog
→ direct composition profile
→ pure decimal calculator
→ immutable in-memory analysis snapshot
```

This is deliberately narrower than the walking skeleton. It proves domain boundaries,
calculation semantics, behavior versioning, unknown-food rejection, and the initial HTTP contract.

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

## Run the foundation API

```powershell
$env:APP_BIND_ADDR = "127.0.0.1:8080"
$env:RUST_LOG = "info"
cargo run -p api-http
```

Health:

```http
GET /health/live
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

## PostgreSQL

Start the local database:

```powershell
docker compose -f deploy/compose.yaml up -d postgres
```

Migration `0001_foundations.sql` creates the seven logical schemas, the minimal walking-skeleton
tables, search indexes, behavior version fields, and immutability guards.

Apply migrations locally:

```powershell
$env:DATABASE_URL = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition"
$env:RUN_MIGRATIONS = "true"
cargo run -p worker
```

Database migrations are forward-only. Do not edit an applied migration; add a new migration.

## Repository boundaries

- `domain`: IDs, units, evidence semantics, pure deterministic calculator.
- `application`: use cases and ports.
- `adapters`: fixture parser/catalog/repository used for local development and tests.
- `persistence-postgres`: PostgreSQL connection and migration adapter.
- `api-http`: Axum HTTP process.
- `worker`: PostgreSQL-backed worker process foundation.

See [`docs/FOUNDATION_DECISIONS.md`](docs/FOUNDATION_DECISIONS.md) for decisions and deferred scope.
The initial governance artifacts are
[`docs/SOURCE_REGISTER.md`](docs/SOURCE_REGISTER.md),
[`docs/RISK_REGISTER.md`](docs/RISK_REGISTER.md), and the development-only
[`VietnameseMealBench manifest`](fixtures/vietnamese-meal-bench/manifest.json).
