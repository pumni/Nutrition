# Foundation 0.2.0

Status: verified implementation increment  
Previous release: `foundation-0.1.0`

## Delivered

- Unicode NFC, lowercase, diacritic-preserving exact-name normalization.
- Typed catalog release, analysis item, and nutrient identifiers.
- PostgreSQL exact food/profile/nutrient evidence adapter.
- Active catalog release membership for searchable names and selectable profiles.
- Idempotent test-only dataset and catalog release seed with raw provenance.
- Transactional persistence of analysis, revision, items, nutrient results, totals, and outbox.
- Immutable JSON result snapshot with SHA-256 verification on read.
- `GET /v1/nutrition/analyses/{analysis_id}`.
- Database-backed `/health/ready`.
- SQLx migration directory change tracking.
- PostgreSQL 18 integration and HTTP create/read replay suite.

## Schema corrections

The initial anonymous idempotency constraint used `UNIQUE NULLS NOT DISTINCT`, which allowed only
one row where both user ID and idempotency key were null. Migration `0004` replaces it with a
partial expression index applied only when an idempotency key exists.

## Verification

- Workspace format, Clippy with warnings denied, unit tests, and JSON validation.
- Five SQLx migrations applied.
- Test-only seed can be applied repeatedly.
- PostgreSQL integration test can run repeatedly without deleting historical analyses.
- HTTP readiness, create, and read/replay pass.
- Published recipe and completed analysis child mutation tests pass.

## Still deferred

- Natural Vietnamese parser and hosted LLM adapter.
- Household/count/volume portion evidence.
- Recipe calculation.
- Clarification and correction state transitions.
- Production source release and catalog.
- Authentication, authorization, and idempotency HTTP middleware.
