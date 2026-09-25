# Local performance baseline

This harness records local/CI observations for two separate analysis paths. It is a measurement
tool, not an optimization, SLO, or production capacity test. It uses no production credentials and
does not contact a hosted provider. The full run is opt-in because it writes analysis records to its
local PostgreSQL database.

## Paths and workload

The DB-oriented path composes the API router in-process with `FixtureParser` and the seeded
foundation catalog. It compares one seeded item (`2 quả trứng gà luộc`) with two seeded items
(`2 quả trứng gà luộc, 1 bát cơm trắng`). There are 10 warmup requests per scenario; warmup samples
are excluded from reported latency and resolver timing. Each measured request gets a unique
idempotency key. The report includes each response status and safe error code; failed requests are
not removed from latency calculations. Resolver timings cover the individual PostgreSQL food,
portion, and suggestion provider calls. Pool observations use SQLx pool size and idle gauges sampled
around those calls. These are process-local observations, not PostgreSQL server-wide measurements.

The hosted-parser path uses `HostedMealParser` with the existing provider-neutral
`StructuredModel` seam and a test-only deterministic fake. The fake waits 25 ms per model call and
returns a transient `benchmark_transient` error on every 17th call; the parser's bounded retry may
therefore add a second fake call. The endpoint is `.invalid`, and its placeholder key is never used
for a network request. No `LLM_API_KEY` or other external credential is read. The report gives
measured fake-call latency and total API latency separately. Backend-overhead percentiles are the
difference between matching percentile summaries; because those samples are unpaired, this is an
approximate observation rather than a causal decomposition.

Request latency percentiles use nearest rank: sort samples ascending, calculate the one-based rank
`ceil(percent × sample_count)`, and select that rank. The report contract is
[`performance-baseline-0.1.0.json`](../../schemas/performance-baseline-0.1.0.json).

## Predeclared decision rule for #20

The rule is implemented in the harness and committed before running the baseline. Compare the
one-item and two-item DB workloads at the same request count, concurrency, pool size (8), and local
environment. The comparison is eligible only when both workloads contain exactly the requested
number of successful 2xx responses, have no error codes, and measured the expected provider calls:
one item has one food plus one portion resolution per request; two items has two of each; neither
path should request suggestions.

Investigate #20 only when all of the following hold:

- Two-item p95 latency increases by at least `max(25 ms, 25% of one-item p95)`.
- Two-item p99 latency increases by at least `max(50 ms, 25% of one-item p99)`.
- Added measured food/portion/suggestion resolution time is at least
  `max(10 ms/request, 50% of the p95 increase)`.

This threshold asks whether repeated evidence-resolution work materially accompanies tail-latency
growth. Meeting it opens investigation only; it does not authorize batching. A failed request or
unexpected provider call count makes the result inconclusive and requires a repaired rerun. If the
comparison is valid but any threshold is missed, the next action is to close #20 as `not planned`
with the report linked.

## Run locally

From `backend`, use a disposable local/CI PostgreSQL database. The default runner starts the
repository's PostgreSQL service, applies migrations and foundation seeds, then stops that service
when finished:

```powershell
./scripts/run-performance-baseline.ps1 -OutputPath D:\Temp\performance-baseline.json
```

The output parent directory is created when needed. The output file must be outside the repository.
To use already-running loopback PostgreSQL, pass `-UseExistingServices`; that database must already
have migrations and foundation seeds. Optional workload flags are `-RequestCount` (20–2000,
default 100) and `-Concurrency` (1–32, default 4). The runner accepts only a loopback PostgreSQL URL
and defaults to the local credentials used by the repository's development compose service.

The ignored measurement test itself is:

```powershell
cargo test -p api-http --lib performance_baseline::write_local_baseline_report -- --ignored --exact --test-threads=1
```

Cheap deterministic verification runs with `cargo test -p api-http --lib performance_baseline` and
`cargo xtask check`; it does not require PostgreSQL or execute the ignored load measurement.
