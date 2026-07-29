# Security and operations boundary

Status: foundation contract, not production certification

## Authentication

Local and CI use `AUTH_MODE=development` with `Authorization: Bearer dev:<uuid>`. The UUID is
derived only from the trusted header adapter and never accepted from request JSON. Every analysis
read/write checks the immutable `meal_analysis.user_id`. Missing credentials return `401`; another
owner returns `403`.

Production startup is intentionally unsupported until a reviewed OIDC/OAuth adapter validates
issuer, audience, signature, expiry, and role claims.

## Sensitive data

- Meal text, source item text, authorization headers, and database URLs are prohibited in logs.
- HTTP tracing records request metadata without bodies or headers.
- Request bodies are capped at 16 KiB.
- Raw meal text encryption, retention, deletion, and export require product/legal policy before
  collection is enabled.

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
