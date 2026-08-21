# Product API v1

Status: implemented for staging preparation; production traffic and canonical publication remain
gated by the production release policy.

The canonical machine-readable contract is [`openapi/nutrition-api-v1.json`](../../openapi/nutrition-api-v1.json).
It is hand-authored and covered by the `api-http` contract test; no OpenAPI generator dependency is used.

## Approved boundary

- `GET /v1/nutrition/analyses` returns only owner-scoped summaries: `analysis_id`, `status`, `locale`,
  `created_at`, `current_revision_number`, `result_status`, and `quality_label`.
- Listing uses `page_size` (default `20`, maximum `50`) and an opaque HMAC-protected keyset cursor.
  Ordering is `created_at DESC, analysis_id DESC`; the cursor binds the principal, filters, and a
  snapshot boundary and expires after 24 hours. Invalid or mismatched cursors return `400 invalid_cursor`.
- `GET /v1/nutrition/analyses/{analysis_id}/workflow` returns the current revision, state, safe pending
  question/options, and allowed action names. It never returns raw meal text, provider payloads, or auth data.
- Existing create, clarification, and correction response shapes remain unchanged. `Idempotency-Key` is
  required on all three routes, limited to 1–128 printable ASCII characters, and scoped to principal plus
  operation (correction and clarification also include the analysis ID).
- Idempotency hashes use a versioned typed canonical JSON envelope, are retained for 24 hours, replay the
  stored immutable response, and return `409 idempotency_conflict` for a different request under the same key.
- The error envelope remains `{"error":{"code":"stable_code","message":"safe_message"}}`.

No migration or dependency was added. Existing analysis and workflow tables provide the required read
fields. `API_CURSOR_HMAC_SECRET` is required for staging/production and must contain at least 32 bytes;
local/CI use a non-deployment-only fallback for verification. This configuration does not authorize
production traffic, provider activation, catalog activation, or release publication.
