# ADR: Hosted parser provider boundary

**Status:** Accepted for implementation and staging; production provider enablement remains gated.

## Context

Hosted parsing is useful for language structure, but provider output is untrusted and must not become
nutrition evidence. Provider behavior, bounds, and privacy need a durable decision separate from code.

## Decision

Use the approved OpenAI Responses endpoint and model with the exact `parsed-meal-0.1.0` schema,
bounded timeout/response size/retry/circuit behavior, and no automatic provider or model fallback.
Send only the minimum parser envelope. A provider/model change requires a new behavior version,
benchmark evidence, and owner approval.

## Consequences

The application remains provider-neutral; mapping, transport, strict validation, and content-free
telemetry stay inside the hosted adapter. Production requires the separately approved provider
privacy/retention gate.

## Evidence / affected paths

- `docs/architecture/parser.md`
- `crates/adapters/src/hosted_parser/`
- `.claude/rules/parser.md`
