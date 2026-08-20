# Product API v1 proposal

Status: non-canonical proposal; owner decision required before implementation

This document inventories the current HTTP behavior and proposes options for
the M4 product API gap identified in issue #12. It does not establish a public
contract, add routes, expose database/provider internals, or authorize a
runtime change.

## Evidence boundary

The inventory is derived from:

- [`crates/api-http/src/main.rs`](../../crates/api-http/src/main.rs): route
  registration, authentication, ownership, idempotency, body limit, request
  IDs, readiness, and error mapping;
- [`crates/application/src/model.rs`](../../crates/application/src/model.rs):
  serialized request/outcome types;
- [`crates/application/src/ports.rs`](../../crates/application/src/ports.rs):
  application error taxonomy and reader capabilities;
- [`crates/persistence-postgres/src/privacy.rs`](../../crates/persistence-postgres/src/privacy.rs):
  the existing user export/deletion shapes;
- [`docs/FOUNDATION_DECISIONS.md`](../FOUNDATION_DECISIONS.md) and
  [`docs/SECURITY_AND_OPERATIONS.md`](../SECURITY_AND_OPERATIONS.md):
  ownership, privacy, versioning, and idempotency boundaries.

The current API is the foundation `0.6.0` behavior. Existing behavior remains
the compatibility baseline even where this proposal identifies an awkward
shape.

## Current route inventory

All authenticated routes require `Authorization: Bearer ...`. Local/CI
development authentication accepts `Bearer dev:<uuid>`; non-development
authentication uses the configured provider-neutral OIDC adapter. Missing or
invalid credentials map to `401 unauthorized`. Analysis resource routes check
PostgreSQL ownership before reading or mutating data; another owner maps to
`403 forbidden`.

| Method and path | Auth / ownership | Request and idempotency | Success shape | Application errors |
|---|---|---|---|---|
| `GET /health/live` | None | None | `200 {"status":"ok","application_version":"..."}` | Process-startup failures are outside the route envelope. |
| `GET /health/ready` | None | None | `200 {"status":"ready",...}` when `SELECT 1` succeeds; `503 {"status":"not_ready",...}` otherwise | No application error envelope. |
| `POST /v1/nutrition/analyses` | Authenticated; principal becomes `owner_id` | `AnalysisRequest`: `text`, `locale`, optional `mode` (`fast`, `balanced`, `precise`). Optional `Idempotency-Key`, 1–128 ASCII characters. Scope is `user:{principal}:create`; same request replays, different request hash returns conflict. | `200` untagged `AnalysisSnapshot` or `ClarificationAnalysis` | `400 invalid_request`, `401 unauthorized`, `409 idempotency_conflict`, `422 analysis_insufficient`, `500 internal_error`, `503 parser_unavailable`. |
| `GET /v1/nutrition/analyses/{analysis_id}` | Authenticated and owner-checked | Path ID must parse as `AnalysisId` | `200 AnalysisSnapshot` | `400 invalid_request`, `401 unauthorized`, `403 forbidden`, `404 analysis_not_found`, `500 internal_error`. |
| `GET /v1/nutrition/analyses/{analysis_id}/revisions/{revision_number}` | Authenticated and owner-checked | Path ID and `u32` revision number | `200` persisted revision snapshot JSON | `400 invalid_request`, `401 unauthorized`, `403 forbidden`, `404 analysis_not_found`, `500 internal_error`. |
| `POST /v1/nutrition/analyses/{analysis_id}/clarifications` | Authenticated and owner-checked | `ClarificationAnswerRequest`: `expected_revision_id`, `question_id`, `option_id`, optional `mass_g`. No idempotency header handling. | `200 AnalysisSnapshot` after append-only revision | `400 invalid_request`, `401 unauthorized`, `403 forbidden`, `409 stale_clarification` or `revision_conflict`, `422 analysis_insufficient`, `500 internal_error`. |
| `POST /v1/nutrition/analyses/{analysis_id}/corrections` | Authenticated and owner-checked | `CorrectionRequest`: `base_revision_id`, 1–10 unique `item_corrections` with positive `quantity` and non-empty `unit`. Optional `Idempotency-Key`; scope is `user:{principal}:correction:{analysis_id}`. | `200` untagged corrected `AnalysisSnapshot` or `ClarificationAnalysis` | `400 invalid_request`, `401 unauthorized`, `403 forbidden`, `409 idempotency_conflict`, `revision_conflict`, or `stale_clarification`, `422 analysis_insufficient`, `500 internal_error`, `503 parser_unavailable` when correction reparsing is required. |
| `GET /v1/nutrition/me/export` | Authenticated; caller scope only | None | `200` `user-data-export-v1` object containing user-owned analyses, revisions, redacted results, clarification/correction history, and behavior versions | `401 unauthorized`, `500 internal_error`. |
| `DELETE /v1/nutrition/me` | Authenticated; caller scope only | No request body. Server creates an opaque `privacy-delete-*` request reference. | `200` deletion receipt with `event_type`, `deleted_at`, and `request_reference` | `401 unauthorized`, `500 internal_error`. |

The application error envelope is:

```json
{
  "error": {
    "code": "stable_code",
    "message": "current human-readable message"
  }
}
```

The `code` values above are the current stable mapping in `ApiError`. The
current `message` values contain implementation wording and must not be
treated as a stable client contract without an owner decision.

### Cross-cutting current behavior

- The API applies a 16 KiB default body limit to the router.
- `X-Request-Id` is created when absent and propagated when present. Tracing
  is JSON and must not include meal text, authorization headers, raw text, or
  database URLs.
- Framework extractor failures such as malformed JSON and body-limit
  rejections are not converted into the application `ErrorEnvelope`. Their
  framework-generated response body is therefore not a stable product
  contract today.
- Public response types are serde-derived domain/application values. Internal
  fields marked `serde(skip)` or `serde(skip_serializing)` are intentionally
  absent; this proposal does not recommend exposing them.
- The current reader supports one analysis and one revision lookup. There is
  no user-scoped analysis listing or cursor endpoint.

## Concrete client workflow gaps

### History and listing

A client can read a known analysis ID or one known revision, but cannot ask for
the authenticated user's analyses, sort them, filter status/locale, or resume
from a prior page. The privacy export is not a product history endpoint: it is
an ownership-scoped export with privacy-redacted data and a different purpose.

### Stable pagination

No cursor, ordering, page-size limit, snapshot boundary, or invalid-cursor
error exists. Offset pagination would be simple but can skip/duplicate rows as
new analyses arrive. A keyset cursor can be stable, but its ordering fields,
opacity/integrity protection, maximum page size, and retention behavior require
contract decisions.

### Workflow continuation

Create can return either a completed snapshot or a clarification question, and
clarification/correction append revisions. There is no canonical workflow
resource describing the current state, allowed next actions, stale revision
handling, or a client-safe continuation link. Clients must infer continuation
from the untagged response shape and then know the route conventions.

### Errors and retries

Application errors have stable codes, but framework rejections do not share the
same envelope. Idempotency is optional for create and correction, has distinct
scopes, and has no documented retention/expiry contract in the HTTP layer.
Clients need an owner-approved statement of which failures are safe to retry,
how long replay records remain available, and whether a missing key is allowed
for every write.

### Machine-readable contract

There is no canonical OpenAPI or equivalent public contract. Serde structs are
useful implementation evidence, but they do not define path parameters,
headers, authentication schemes, error responses, pagination, or lifecycle
semantics for an external client.

## Non-canonical proposal options

These options are intentionally alternatives, not implementation instructions.

### Resource and history shape

**Option A — extend the existing resource.** Add `GET /v1/nutrition/analyses`
with a user-scoped page object and retain the current single-analysis routes.
This is the smallest URL change and keeps analysis as the primary resource,
but requires careful distinction between a list summary and a full snapshot.

**Option B — add a client history resource.** Add
`GET /v1/nutrition/history` with explicit summaries and links to analysis and
revision resources. This is clearer for clients and leaves the existing
resource path untouched, but creates another public resource name and a
longer-term compatibility surface.

Owner approval is required for the resource name, summary fields, ordering,
filters, maximum page size, and whether behavior-version metadata appears in
list results.

### Pagination

**Option A — opaque keyset cursor.** Order by a stable tuple such as
`created_at, analysis_id`, encode the last tuple in an opaque cursor, and bind
the cursor to the authenticated principal and filter set. This avoids offset
drift and avoids exposing database internals, but requires cursor versioning,
integrity protection, and expiry semantics.

**Option B — offset/page pagination.** Expose page number and bounded page
size. This is easier to implement and inspect, but inserts/deletes can cause
duplicates or skips and a page number is not a stable continuation token.

The proposal recommends evaluating keyset semantics first, but does not select
the cursor fields, encoding, expiry, or ordering policy.

### Workflow continuation

**Option A — tagged outcome and action metadata.** Preserve the existing data
fields and add an explicit non-ambiguous `outcome`/`next_actions` wrapper. This
helps clients distinguish completed vs clarification states, but changes the
create/correction response envelope and therefore needs a versioning plan.

**Option B — workflow resource.** Keep existing response shapes and add a
workflow/status resource that exposes current revision, pending question,
allowed action names, and resource links. This minimizes changes to existing
responses, but adds a new read contract and consistency rules.

Either option must preserve append-only revisions, expected-revision checks,
ownership, and the existing behavior-version vector. The owner must decide
whether pending clarification context can be exposed beyond the current
question/options shape.

### Error and idempotency contract

**Option A — version the current envelope.** Keep `{error:{code,message}}`,
document the existing codes, and add a machine-readable details field only in a
new API version. This is a small conceptual change but requires a decision on
which details are safe to expose.

**Option B — problem-details style envelope.** Adopt a standard media type with
stable type/title/status/code/instance fields. This is more interoperable, but
changes client parsing and requires mapping framework extractor failures too.

For either option, the owner must approve required-vs-optional idempotency for
each mutating endpoint, key scope, request-hash canonicalization, replay
retention/expiry, conflict behavior, and retry guidance. No proposal choice
changes the current runtime.

### Machine-readable source

**Option A — hand-authored OpenAPI.** Treat a reviewed OpenAPI document as the
canonical public contract and test the implementation against it. It is easy
for clients and tooling, but can drift from serde types unless contract tests
enforce parity.

**Option B — generated contract plus reviewed overlays.** Generate schemas
from Rust types and maintain reviewed path/auth/error/pagination overlays. This
reduces shape drift, but the build and dependency strategy become part of the
contract workflow.

The owner must approve the canonical artifact, versioning policy, publication
location, and whether adding a generation dependency is permitted. This task
does not add OpenAPI or another dependency.

## Smallest owner approvals before runtime work

Before an implementation intent can add M4 routes or change existing response
envelopes, the owner must approve:

1. canonical resource names and endpoint list for history/listing and workflow
   continuation;
2. fields allowed in summaries, pages, workflow state, links, and metadata,
   including the privacy boundary and whether behavior versions are exposed;
3. pagination order, cursor/page model, limits, filter semantics, invalid
   cursor behavior, and snapshot consistency expectations;
4. workflow state transitions, allowed next actions, stale revision behavior,
   and whether existing untagged outcomes are preserved or versioned;
5. stable error media type/envelope, framework rejection mapping, error-code
   taxonomy, safe message/details policy, and HTTP status guarantees;
6. idempotency requirements, scope, hash/replay retention, expiry, and retry
   semantics for every mutating route;
7. canonical machine-readable contract artifact and its version/publication
   policy.

Until these decisions are recorded in a new approved Task Intent, this file is
only a proposal. No route, schema, dependency, migration, response status, or
persistence behavior is changed by it.
