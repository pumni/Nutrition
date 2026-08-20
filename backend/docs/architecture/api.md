# HTTP API architecture

`api-http` is a composition edge. It maps HTTP requests and responses, applies authentication and
ownership boundaries, and assembles application ports with adapters and PostgreSQL persistence.
Business semantics remain in `application` and `domain`.

Development bearer authentication is local/CI-only. Staging and production use the provider-neutral
OIDC boundary. Tokens, claims, meal content, and provider responses are never logged.

The current public surface is documented in [Product API v1](../product/api-v1.md). Use `cargo
xtask postgres` for API readiness, replay, idempotency, authentication, and ownership checks.
