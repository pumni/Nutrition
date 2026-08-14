# Nutrition backend

Evidence-first nutrition analysis backend with current decisions recorded in
[`docs/FOUNDATION_DECISIONS.md`](docs/FOUNDATION_DECISIONS.md).

Current behavior release: `foundation-0.6.0`.

## Implemented foundation slice

```text
quantity + unit + food
→ explicit fixture parser or bounded hosted-parser anti-corruption adapter
→ exact PostgreSQL catalog lookup
→ explicit mass or release-scoped portion observation
→ direct composition profile
→ pure decimal calculator with propagated bounds
→ transactional PostgreSQL analysis snapshot
→ hash-verified read/replay
→ one-turn clarification or append-only correction revision
```

This slice proves domain boundaries, calculation semantics, behavior versioning,
unknown-food rejection, PostgreSQL transaction boundaries, immutable revision history,
clarification/correction state transitions, idempotent create/correction replay, and a fail-closed
hosted parsing boundary.

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
$env:AUTH_MODE = "development"
$env:PARSER_MODE = "fixture"
$env:RUST_LOG = "info"
cargo run -p api-http
```

`AUTH_MODE=development` accepts only `Authorization: Bearer dev:<uuid>` and exists for local/CI
contract testing. Any other auth mode fails startup until a production OIDC adapter is provided.
`PARSER_MODE` is also required: `fixture` is local/CI only, while `hosted` requires an approved
provider configuration and never silently falls back to fixture behavior.

Hosted mode uses a provider-neutral HTTPS envelope:

```powershell
$env:PARSER_MODE = "hosted"
$env:LLM_ENDPOINT = "https://approved-provider.example/v1/meal-parse"
$env:LLM_API_KEY = "<secret>"
$env:LLM_PROVIDER = "<provider-version>"
$env:LLM_MODEL = "<model-version>"
```

Optional bounded settings are `LLM_TIMEOUT_MS` (default 3000),
`LLM_MAXIMUM_RESPONSE_BYTES` (65536), `LLM_CIRCUIT_FAILURE_THRESHOLD` (5), and
`LLM_CIRCUIT_COOLDOWN_SECONDS` (30). See
[`docs/HOSTED_PARSER.md`](docs/HOSTED_PARSER.md) for the transport contract and privacy boundary.

Health and readiness:

```http
GET /health/live
GET /health/ready
```

Foundation analysis request:

```http
POST /v1/nutrition/analyses
Authorization: Bearer dev:0198f100-0000-7000-8000-000000000098
Idempotency-Key: <opaque-key>
Content-Type: application/json

{
  "text": "2 quả trứng gà luộc, 1 bát cơm trắng",
  "locale": "vi-VN",
  "mode": "balanced"
}
```

Only two fixture foods are available. The parser accepts
`<quantity> <unit> <food>`: grams resolve directly, while `quả` for boiled egg and `bát` for white
rice resolve through test-only portion observations with lower and upper mass bounds. A known
single food with an unsupported unit returns `needs_clarification`; unknown foods still return
`analysis_insufficient` and are never force-matched.

Read the current persisted revision:

```http
GET /v1/nutrition/analyses/{analysis_id}
GET /v1/nutrition/analyses/{analysis_id}/revisions/{revision_number}
POST /v1/nutrition/analyses/{analysis_id}/clarifications
POST /v1/nutrition/analyses/{analysis_id}/corrections
```

The read path verifies the persisted snapshot SHA-256 before deserialization.

## PostgreSQL

The eleven migrations create seven logical schemas, the minimal walking-skeleton tables, search
indexes, behavior version fields, snapshot persistence, scoped idempotency, release membership,
workflow state enforcement, worker leases, audit storage, ownership, parser invocation telemetry,
and immutability guards.

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
- `adapters`: deterministic fixture parser, bounded hosted-parser adapter, and in-memory test
  doubles.
- `persistence-postgres`: migrations, exact catalog lookup, contextual portion lookup,
  transactional analysis repository, snapshot reader, and explicit test-only seed.
- `api-http`: Axum HTTP process.
- `worker`: PostgreSQL-backed worker process foundation.

Worker modes are `idle`, `run-once`, and `loop`. `run-once` is used in verification; `loop` adds
bounded polling and graceful shutdown. The current outbox target is an explicit foundation test
sink, not an external broker.

See [`docs/FOUNDATION_DECISIONS.md`](docs/FOUNDATION_DECISIONS.md) for decisions and deferred scope.
The initial governance artifacts are
[`docs/SOURCE_REGISTER.md`](docs/SOURCE_REGISTER.md),
[`docs/RISK_REGISTER.md`](docs/RISK_REGISTER.md), and the development-only
[`VietnameseMealBench manifest`](fixtures/vietnamese-meal-bench/manifest.json).

## AI coding context layer

`AGENTS.md` is the repository entrypoint for coding agents. A human supplies Task Intent; the
trusted harness compiles the execution spec; the coding agent operates with policy-bounded
implementation autonomy and must not make protected project decisions. This governance layer does
not alter nutrition runtime behavior.

Run the context self-test, the default context verification, and the full foundation verification:

```powershell
.\scripts\verify-agent-context.ps1 -SelfTest
.\scripts\verify-agent-context.ps1
.\scripts\verify.ps1
```

Modern work starts from the [Task Intent example](.agent/templates/task-intent.example.json). The
trusted runner compiles it into a Task Spec, discovers relevant context from repository evidence,
and derives applicable gates from the final diff.
