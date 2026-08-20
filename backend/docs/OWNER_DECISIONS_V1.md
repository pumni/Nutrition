# Nutrition Backend — Owner Decisions v1

**Status:** Owner-approved for implementation and staging preparation; production activation is not approved.

**Accepted:** 2026-08-20 by the repository owner through the execution handoff for this repository.

**Execution order:** `P1-101 → P1-102 → P1-103 → P2-104 → P2-105 → P0-106`

This document is the durable owner decision source for the approved implementation boundary. It does
not authorize production traffic, production catalog activation, or canonical `v1.0.0` publication.

## OWNER-BE-001 — Hosted parser provider

- Provider: **OpenAI API**.
- Endpoint: `https://api.openai.com/v1/responses`.
- Exact v1 model: **`gpt-5.6-luna`**.
- Purpose: language parsing only.
- Structured output: exact `parsed-meal-0.1.0` JSON Schema.
- No automatic fallback to a different model or provider.
- A provider/model change requires a new behavior version, Vietnamese benchmark, and owner approval.
- Per-attempt timeout: `5000 ms`.
- Maximum attempts: `2` total.
- Maximum response: `65536` bytes.
- Circuit breaker: `5` consecutive failures; `30 s` cooldown.
- Retry once on transient network/timeout/429/5xx or schema-invalid output.
- Do not retry semantic failures or permanent 4xx.

### Privacy boundary

Send only locale, untrusted meal text, the fixed system instruction, and the schema. Never send user
identity, authorization material, meal history, resolved food IDs, nutrition results, or source URLs.

Implementation and staging are approved. Production hosted parsing requires Zero Data Retention for
the provider account, when available and approved, or a separately recorded owner acceptance of the
provider retention policy. Until that gate is closed, staging uses benchmark/synthetic or explicitly
approved test text.

## OWNER-BE-002 — Initial Vietnamese catalog scope

The first vi-VN curation scope is the versioned VietnameseMealBench-derived corpus plus reviewed
evidence packages produced by `Nutrition_data_factory`. The first activatable composition slice
remains the already-approved exact 20-record FDC Foundation allowlist until
`PRODUCTION_DATA_STRATEGY` is explicitly amended. Additional Vietnamese identities may be prepared
as staged/draft candidates, but they are not production-eligible merely by this decision.

- Composition source remains approved FDC Foundation evidence unless a later source-rights decision is recorded.
- Runtime fuzzy resolution remains prohibited.
- Only exact identities and **human-approved** aliases may become production mappings.
- Composite dishes require reviewed recipe evidence with exact ingredient identities/quantities and cooked yield/output evidence.
- Household/count units use exact source-backed portion evidence when context matches; otherwise reviewed project measurements.
- Unsupported recipe/portion/identity combinations clarify or remain insufficient; they are never guessed.
- Benchmark engineering-fixture gram values must not become production portion evidence.
- The prohibited Vietnam FCT 2017 source must not be ingested.
- AI-generated aliases, recipes, yields, portions, and compositions are proposals only.

## OWNER-BE-003 — Product API v1

Canonical contract: `openapi/nutrition-api-v1.json`, hand-authored and reviewed, with contract tests.
Prefer existing `serde_json` tooling; do not add an OpenAPI generator dependency.

Retain existing endpoints and add:

- `GET /v1/nutrition/analyses`
- `GET /v1/nutrition/analyses/{analysis_id}/workflow`

Listing is owner-scoped and exposes only:
`analysis_id`, `status`, `locale`, `created_at`, `current_revision_number`, `result_status`, and
`quality_label`. It must not expose raw meal text, source spans, provider payloads, or authorization data.

Filters are `status` and `locale`; default page size is `20`, maximum `50`.

Pagination uses an opaque HMAC-protected keyset cursor ordered by
`created_at DESC, analysis_id DESC`. The first page pins a snapshot boundary. The cursor is bound to
principal, filters, and snapshot, expires after 24 hours, and invalid cursors return
`400 invalid_cursor`.

Workflow is a separate status resource so existing create/correction response shapes do not change.
It exposes current revision, state, current safe pending question/options, and allowed actions.

Keep the error envelope:

```json
{"error":{"code":"stable_code","message":"safe_message"}}
```

Malformed JSON, body-limit, and framework extractor failures use the same envelope. Keep
`X-Request-Id`; do not add an arbitrary `details` field in v1.

Require `Idempotency-Key` for create, clarification, and correction POST routes. Keys are 1–128
printable ASCII characters and retained for 24 hours. Same key plus the same versioned typed canonical
request replays; same key plus a different request returns `409 idempotency_conflict`. Hash the
versioned typed canonical JSON, never raw HTTP bytes.

## OWNER-BE-004 — SLO and observability

Rolling 30-day launch targets:

- Non-hosted API availability: `99.9%`.
- Hosted end-to-end analysis availability: `99.5%`.
- Read p95/p99: `300 ms`/`800 ms`.
- Non-hosted mutation p95: `750 ms`.
- Hosted analysis p95/p99: `6 s`/`10 s`.
- Readiness p95: `200 ms`.

Expected `4xx`, `needs_clarification`, and contract-valid `insufficient_evidence` are not availability failures.

Metrics cover HTTP, parser, DB pool, worker queue, outbox, catalog release, and privacy retention.
Logs remain structured JSON and content-free for meal, auth, and secret data.

## OWNER-BE-005 — Backup / RPO / RTO

- PostgreSQL: encrypted daily backup plus continuous WAL/PITR where supported.
- RPO: `15 minutes`.
- RTO: `4 hours`.
- Backup retention: `35 days`.
- Restore drill: monthly in staging and at least quarterly from production backup copies.
- Catalog/source packages remain immutable and checksum-bound in durable object storage.
- Restores reapply privacy deletion/retention tombstones before restored user data serves traffic.

## OWNER-BE-006 — Production gate

This decision set authorizes implementation and staging preparation, **not production activation**.
Production still requires:

1. Provider privacy/retention gate satisfied.
2. Vietnamese benchmark threshold approved and passed.
3. Catalog package explicitly `production_eligible=true` with human evidence.
4. Staging SLO/load/restore evidence reviewed.
5. Release manifest and rollback target reviewed.

The owner remains the sole authority for production catalog activation, production traffic, release tag,
and canonical publication.

## OWNER-BE-007 — Single-owner P1-101 staging/merge waiver

**Status:** Owner-approved on 2026-08-20 for the P1-101 staging/merge gate only.

Because this is a single-owner project, the owner approves a narrowly scoped waiver of the normal
two-independent-human benchmark evidence requirement for the P1-101 implementation/staging merge
gate. The waiver is not a benchmark pass and does not make an AI subagent an annotator or adjudicator.

- The waiver applies only to task `INTENT-P1-101` and gate `benchmark-external`.
- A trusted evidence wrapper may use `result: "waived"` with a matching owner-waiver artifact.
- The artifact must identify this decision, the exact target commit, the waiver scope, and
  `production_authorization: false`.
- Any machine-generated review or prediction is machine evidence only and must not be labeled human
  annotation, human adjudication, or production benchmark evidence.
- P1-101 may proceed through implementation/staging merge review under this waiver, but production
  hosted parsing remains blocked until the provider privacy/retention gate and the production
  Vietnamese benchmark requirements in `OWNER-BE-006` are satisfied.
- This waiver does not apply to P1-102, P1-103, P2-104, P2-105, P0-106, production activation,
  catalog release, real-user traffic, or canonical publication.

## Source integrity

This repository record was imported from the owner decision package
`nutrition_backend_owner_decisions_v1` after explicit owner acceptance. The source package hashes were:

- `OWNER_DECISIONS_V1.md`: `9ccbff49d1e41be8cfc74e44ac7f081e2220900a8531b270c1b87fd6e1a537f4`
- `owner-decisions.json`: `2df972a27fa7855d7571992885761e68965e81c296042706cc68d37f8302ea42`
