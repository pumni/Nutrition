# ADR: Privacy-safe observability

**Status:** Accepted for implementation and staging verification.

## Context

Operational visibility is required without turning logs, labels, or metric values into a second
copy of meal, identity, credential, or provider data.

## Decision

Use the bounded Prometheus-compatible contract already implemented by the workspace. Labels are
low-cardinality operation dimensions such as route class, status class, parser outcome, database
operation, worker job, catalog release, and retention outcome. Request IDs are correlation fields,
never metric labels. Raw content and arbitrary paths are prohibited.

## Consequences

Alert and staging artifacts remain operational evidence, not production authorization. Further
telemetry dependencies or label changes require a new owner decision.

## Evidence / affected paths

- `docs/operations/observability.md`
- `deploy/observability/`
- `crates/api-http/src/observability.rs`
