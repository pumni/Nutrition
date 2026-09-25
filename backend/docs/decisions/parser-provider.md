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

The application remains provider-neutral. Inside `crates/adapters`, the parser owns prompt
semantics, the strict output schema and its schema/semantic validation, grounding, repair policy,
privacy, and content-free parser telemetry. Provider implementations map the neutral structured
request/response and classified errors to their protocol, and enforce transport bounds. The current
OpenAI Responses mapping remains behind that SPI. Production requires the separately approved
provider privacy/retention gate.

## Evidence / affected paths

- `docs/architecture/parser.md`
- `crates/adapters/src/hosted_parser/`
- `.claude/rules/parser.md`
