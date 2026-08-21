# Security and operations boundary

Status: foundation contract, not production certification

## Authentication

Local and CI use `AUTH_MODE=development` with `Authorization: Bearer dev:<uuid>`. The UUID is
derived only from the trusted header adapter and never accepted from request JSON. Every analysis
read/write checks the immutable `meal_analysis.user_id`. Missing credentials return `401`; another
owner returns `403`.

Staging and production use the provider-neutral OIDC adapter with an HTTPS configured issuer and
audience. It validates exact issuer, expected audience, `RS256` signatures from discovered JWKS,
expiry, and optional not-before claims with a 60-second clock-skew allowance. It never uses email
as identity, does not enable role/scope authorization in v1, refreshes an unknown JWKS key ID once,
and fails closed when no fresh matching key is available. `(issuer, subject)` is mapped transactionally
to a UUIDv7 internal user identity.

## Sensitive data

- Meal text, source item text, authorization headers, and database URLs are prohibited in logs.
- HTTP tracing records request metadata without bodies or headers.
- Request bodies are capped at 16 KiB.
- Raw meal text persistence remains disabled. Retention, deletion, and export must follow the
  approved privacy contract and must not add meal content to logs or telemetry.
- `GET /v1/nutrition/me/export` returns the versioned `user-data-export-v1` user-owned export.
- `DELETE /v1/nutrition/me` is ownership-scoped and returns only a deletion event type, timestamp,
  and opaque request reference. It removes external identity mapping only after owned data purge;
  global catalog/composition evidence is retained.
- Prometheus-compatible operational metrics are exposed only on the separately configured internal
  metrics listener. Labels use normalized route/operation classes and never contain meal text,
  tokens, authorization material, user IDs, analysis IDs, provider payloads, database URLs, or raw
  request paths. See [the observability runbook](observability.md).

## Worker

- `WORKER_MODE=idle`: migration/seed/healthcheck process.
- `WORKER_MODE=run-once`: one bounded job and outbox batch for CI/cron.
- `WORKER_MODE=loop`: bounded polling until Ctrl-C graceful shutdown.
- Claims use `SKIP LOCKED`; attempts are bounded; terminal failures use `dead`.
- Unsupported job types fail closed and never execute arbitrary payload code.

The outbox publisher currently marks delivery to a database-local verification sink. Before an
external broker/webhook is enabled, add transport idempotency, retry classification, dead-letter
inspection, credentials from a secret manager, and crash-injection tests.

## Initial incident actions

For suspected credential, meal-history, or catalog exposure: disable the affected adapter, preserve
audit evidence, rotate credentials, identify affected analysis IDs, and follow the reviewed
notification/retention policy. This document is not a substitute for a production incident plan.
