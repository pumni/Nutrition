# Foundation 0.5.0

Status: verified implementation increment  
Previous release: `foundation-0.4.0`

## Delivered

- PostgreSQL `SKIP LOCKED` bounded job claiming.
- Attempt counting, quadratic capped backoff, completion, retry, and dead-letter transitions.
- Bounded outbox run-once delivery to an explicit foundation test sink.
- Worker `idle`, `run-once`, and continuous loop modes with graceful shutdown.
- Typed worker environment bounds for pool, batch, and polling interval.
- Immutable analysis ownership.
- Development bearer authentication and object-level authorization.
- `401` missing-auth and `403` cross-owner HTTP contracts.
- 16 KiB API body limit and fail-fast non-development auth configuration.
- Audit-event schema and prohibited-sensitive-log static verification.
- Security and operational boundary documentation.

## Production boundary

The development bearer adapter is local/CI only and deliberately prevents production-mode startup.
The outbox test sink does not represent external exactly-once delivery. OIDC validation, runtime DB
roles/TLS, secret management, external transport idempotency, retention/deletion, backup restore,
and operational alerting remain production gates.

## Verification

- Ten forward-only migrations.
- Workspace formatting, Clippy warnings denied, unit tests, and sensitive-log scan.
- Concurrent-safe job claim, completion, terminal failure, and outbox integration.
- Worker run-once smoke.
- Authenticated create/read/clarification/correction/history.
- Missing and foreign principal rejection.
- Owner immutability and running-job lease SQL guards.

## Next increment

`foundation-0.6.0` introduces the hosted LLM parser adapter, strict output validation, retry budget,
circuit breaker, and provider privacy contract.
