# Foundation 0.6.0

Status: verified implementation increment  
Previous release: `foundation-0.5.0`

## Delivered

- Explicit `fixture|hosted` parser selection with fail-fast startup and no silent fallback.
- Provider-neutral HTTPS parser anti-corruption adapter.
- Fixed prompt and strict versioned output schema.
- Bounded input, timeout, streamed response size, and TLS-only endpoint validation.
- Strict provider envelope, JSON Schema, typed, and semantic validation.
- Source-grounding, duplicate, negated-consumption, and prompt-injection warning controls.
- One retry maximum for transient transport/timeout or schema repair only.
- Provider/model circuit breaker with bounded threshold and cooldown.
- Privacy-minimized request DTO and mock-provider assertions.
- Non-content PostgreSQL telemetry for versions, latency, retry, tokens, output hash, and status.
- Persisted exact hosted prompt/schema/provider/model behavior vector.

## Production boundary

This increment completes the planned engineering foundation, not production readiness. The hosted
adapter defines a gateway contract but does not endorse or hard-code a provider. Provider-specific
mapping, privacy/legal approval, data residency and retention, secrets, staging Vietnamese
benchmark thresholds, load testing, alerting, OIDC, and production catalog evidence remain release
gates.

## Verification

- Eleven forward-only migrations.
- Workspace formatting, Clippy warnings denied, and all unit tests.
- Mock-provider success, schema repair, semantic fail-closed, transient retry, circuit breaker,
  minimum-data request, and non-raw telemetry assertions.
- PostgreSQL migration/integration, API smoke/replay/auth/ownership, worker, and SQL immutability
  verification.

## Foundation handoff

The six foundation increments now provide the backend architecture, deterministic evidence and
calculation path, immutable workflow history, security/worker operations, and a constrained hosted
parser boundary. Subsequent work should proceed as product/data vertical slices with explicit
production gates rather than adding more foundation infrastructure by default.
