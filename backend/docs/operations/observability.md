# Observability contract

Status: implementation and staging preparation only. Production activation remains closed under
`docs/decisions/production-gate.md#adr-production-activation-gate`.

## Runtime boundary

P2-104 uses the owner-approved Prometheus-compatible pull model from OWNER-BE-009. The API and,
when configured for a long-running deployment, the worker expose `/metrics` on dedicated
operational listeners. The listener must be bound to an internal interface or network. It must not
be exposed as a public product endpoint.

Required staging configuration:

```text
API_METRICS_BIND_ADDR=0.0.0.0:9090
WORKER_METRICS_BIND_ADDR=0.0.0.0:9091
```

Local and CI API runs default to `127.0.0.1:9090`. Local and CI worker runs install an in-process
recorder without opening a listener so bounded run-once and idle verification processes exit.
Staging/production requires an explicit worker metrics bind address.

The versioned scrape and alert artifacts are:

- `deploy/observability/prometheus.yml`;
- `deploy/observability/nutrition-alerts.yml`;
- `deploy/observability/staging-alert-tests.yml`.

They define configuration artifacts only. P2-104 does not deploy Prometheus, an alert receiver, a
dashboard, or a production collector.

Runtime histograms use explicit bounded buckets so the SLO rules can evaluate Prometheus
`histogram_quantile` expressions. The bucket set includes the OWNER-BE-004 boundaries at `0.2`,
`0.3`, `0.75`, `0.8`, `6`, and `10` seconds, plus bounded operational headroom.

## Privacy and cardinality

Metric labels are limited to the fixed method set (`GET`, `POST`, `DELETE`, `OTHER`), normalized route class, status class, outcome, parser mode,
retry class, parser error class, database operation, worker job class, and operation outcome.
Request IDs are correlation fields in structured JSON request spans and response headers, never
metric labels. Client-supplied non-UUID request IDs are hashed for logs while the response header
continues to preserve the approved propagated value. Raw paths are normalized before metrics are
emitted. Meal text, tokens, authorization
material, user IDs, analysis IDs, provider payloads, database URLs, and arbitrary job payloads are
not emitted by this contract.

Expected `4xx` responses, `needs_clarification`, and contract-valid insufficient evidence remain
product outcomes; they are not counted as API availability failures. Server-error rates and
latency histograms are the availability/SLO signals.

## Alert actions

| Alert | OWNER-BE-004 signal | First owner action |
| --- | --- | --- |
| `NutritionApiErrorBudgetBurn` | Non-hosted API availability | Check recent deploy/config/provider errors; stop promotion if sustained. |
| `NutritionReadP95AboveTarget` / `NutritionReadP99AboveTarget` | Read p95/p99 and readiness p95 | Inspect DB pool gauges and query latency; compare against the staging load report. |
| `NutritionMutationP95AboveTarget` | Non-hosted mutation p95 | Check DB pool pressure, idempotency waits, and request volume; do not loosen the SLO. |
| `NutritionHostedAnalysisP95AboveTarget` / `NutritionHostedAnalysisP99AboveTarget` | Hosted analysis p95/p99 | Check parser timeout/retry/circuit metrics and provider privacy/availability evidence. |
| `NutritionParserCircuitOpen` | Parser failure protection | Stop hosted promotion, inspect bounded provider failures, and verify cooldown recovery. |
| `NutritionWorkerDeadJobs` | Worker queue health | Inspect the bounded job class and last safe error; replay only through the reviewed runbook. |
| `NutritionDatabaseReadinessFailure` | Readiness p95 and DB availability | Check PostgreSQL health, pool saturation, and restore/backup status before promotion. |

Alert annotations contain only stable action references; they do not contain request, user,
analysis, meal, provider, or database identifiers.

## Staging verification scenarios

Use the exact target commit and an owner-approved Prometheus image digest in the staging evidence.
Run the rule syntax and failure fixtures with the image's `promtool`:

```text
promtool check rules deploy/observability/nutrition-alerts.yml
promtool test rules deploy/observability/staging-alert-tests.yml
```

The fixtures cover a parser circuit-open event, a sustained database-readiness failure, and an API
server-error budget burn. The staging run must additionally scrape both listeners and record:

1. a successful health/readiness request and its normalized HTTP labels;
2. a parser success plus a bounded parser failure/circuit event, without raw text or payload fields;
3. a worker batch with no dead jobs and a controlled unsupported-job failure in a disposable staging
   database, if the staging owner permits that fixture;
4. DB pool gauges and readiness outcomes;
5. the SHA-256 of the captured telemetry/rule evidence.

This is staging evidence only. It does not satisfy the benchmark, provider privacy, catalog
`production_eligible`, restore, SLO review, release-manifest, rollback, or production approval
gates.
