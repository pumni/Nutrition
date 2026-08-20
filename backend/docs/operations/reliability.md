# Reliability baseline harness

`scripts/run-reliability-baseline.ps1` is a local-only observation harness for the existing
foundation seams. It does not set an SLO, declare a capacity limit, change pool or retry
configuration, or call the hosted parser.

The caller must choose the test-wave concurrency explicitly. That number is a repeatability
input for this probe, not a production target. Example:

```powershell
$report = Join-Path $env:TEMP "nutrition-reliability-baseline.json"
pwsh -NoLogo -NoProfile -File .\scripts\run-reliability-baseline.ps1 `
  -Concurrency 4 `
  -OutputPath $report
Get-Content -Raw $report
```

The default run starts the repository's local PostgreSQL 18 Compose service, applies the
existing migrations and foundation fixture, runs the ignored PostgreSQL integration suite,
runs the hosted-parser timeout/circuit unit probes against local test doubles, builds the API
with `APP_ENV=ci`, `AUTH_MODE=development`, and `PARSER_MODE=fixture`, and then runs these
local HTTP waves:

- concurrent create requests sharing one idempotency key;
- concurrent corrections using one base revision and distinct idempotency keys;
- concurrent answers for one open clarification.

Each HTTP wave records status counts, error codes, and the observed convergence or conflict
shape. For example, a single successful concurrent idempotency winner followed by concurrent
`internal_error` responses is reported as `winner_with_concurrent_failures`; a later sequential
replay does not hide that observation. A behavioral observation is not converted into a production claim. The report is written
outside the repository and includes explicit `hosted_provider_called=false` and
`production_credentials_used=false` fields.

Use `-UseExistingServices` only when a caller-managed local API and PostgreSQL are already
running with the same development configuration. The API URL and database URL are fail-closed
to loopback hosts. The harness stops only the PostgreSQL service it started and terminates only
the API process it started.

This harness supplements, rather than replaces, the canonical gates. Review the generated
report together with `cargo xtask check` and `cargo xtask postgres`; neither a passing
harness process nor a single local observation is production release approval.
