# ADR: Benchmark and verification evidence

**Status:** Accepted for staging and repository verification.

## Decision

Local benchmark tooling may prepare and compare machine evidence, but it must not label machine
output as human annotation/adjudication or claim production eligibility. Verification harnesses may
exercise the approved API contract and database behavior without changing runtime or production
authorization.

## Evidence / affected paths

- `docs/evidence/vietnamese-meal-bench-adjudication.md`
- `docs/operations/staging-release-gate.md`
- `evals/coding-agent/`
