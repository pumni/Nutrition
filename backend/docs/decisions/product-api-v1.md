# ADR: Product API v1

**Status:** Accepted current contract.

## Context

The public API needs stable, owner-scoped read and workflow surfaces without exposing raw meal,
provider, authorization, or database details.

## Decision

The hand-authored `openapi/nutrition-api-v1.json` is canonical. Preserve the existing envelope and
request ID behavior, require bounded idempotency keys on mutations, use owner-scoped keyset
pagination with an HMAC cursor, and expose workflow as a separate status resource.

## Consequences

Implementation, contract tests, and docs must change together. A key reused with a different typed
canonical request conflicts; an identical request replays the immutable result.

## Evidence / affected paths

- `openapi/nutrition-api-v1.json`
- `docs/product/api-v1.md`
- `crates/api-http/src/`
