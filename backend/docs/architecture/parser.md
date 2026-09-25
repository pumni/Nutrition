# Hosted parser contract

Status: foundation transport contract  
Behavior release: `foundation-0.6.0`

## Purpose

The hosted model is a constrained language parser only. Nutrition values, food resolution,
portion mass, composition selection, and calculation remain deterministic backend responsibilities.
The model must not return calories, nutrients, internal IDs, URLs, or inferred gram weights.

The hosted parser calls a provider-neutral structured-generation SPI in `crates/adapters`. Its
request contains typed provider/model identities, the parser-owned system instruction, one opaque
untrusted input string, and the parser-owned strict JSON Schema. The response contains a structured
JSON value and bounded token-usage metadata; failures carry a transient/permanent classification and
a content-free code. The SPI contains no provider SDK or wire types.

The parser owns prompt semantics, schema definition and validation, nutrition-specific semantic
validation, grounding, repair policy, privacy, timeout/retry/circuit policy, and parser telemetry.
A provider implementation owns mapping the neutral request and response to its protocol, HTTP status
classification, credential headers, redirect policy, and response-size bounds. The concrete
`OpenAiResponsesProvider` maps the SPI to the OpenAI Responses API at
`https://api.openai.com/v1/responses` using provider `openai` and model `gpt-5.6-luna`. It must not
fall back to another provider or model.

`ProviderModelBulkhead` is a separate decorator around the selected model. It bounds in-flight calls
for that resolved provider/model pair, rejects immediately when saturated, and keeps no waiter queue.
Capacity rejection is non-retryable and does not count as provider failure for the circuit breaker.

At startup, API composition resolves server-side provider and model settings through the static
`StructuredModelProviderRegistry`. It rejects uninstalled providers, unsupported models, missing
secrets, and endpoints outside the installed provider definition. The resolved provider/model pair
also supplies the behavior metadata; clients cannot select it.

## Request envelope

The parser passes a structured-generation request containing:

- typed provider and exact model identities;
- a fixed system instruction, with the single schema-repair instruction added by the parser only on
  the schema-repair retry;
- the exact `parsed-meal-0.1.0` strict JSON Schema;
- one untrusted input string containing the locale and meal text.

The current OpenAI mapping turns that input into the Responses API user message and maps the schema
to strict JSON-schema output formatting. Those wire details stay behind the provider implementation.

No user ID, authorization header value, account metadata, meal history, resolved food ID,
nutrition result, or source URL is part of the SPI request. The bearer secret exists only in the
transport header and is never included in telemetry. HTTP redirects are disabled so meal text
cannot be forwarded to an endpoint other than the explicitly configured HTTPS URL.

## Response envelope

```json
{
  "output": {
    "language": "vi",
    "items": [
      {
        "source_text": "2 quả trứng gà luộc",
        "food_phrase": "trứng gà luộc",
        "quantity": 2,
        "unit_phrase": "quả",
        "modifiers": ["luộc"]
      }
    ],
    "warnings": []
  },
  "input_tokens": 20,
  "output_tokens": 30
}
```

The provider mapping accepts only the structured output and bounded metadata used by the parser.
Token fields are optional but cannot be negative. The response is streamed into a buffer with a
configurable hard limit; declared and actual oversized responses fail closed.

## Validation and resilience

Validation order is:

1. strict JSON envelope;
2. strict versioned JSON Schema with `additionalProperties: false`;
3. typed deserialization;
4. source-span and food-phrase grounding;
5. negated-consumption and duplicate rejection;
6. deterministic unit normalization.

One retry is allowed only after a transient connection/timeout/429/5xx failure or schema-invalid
output. Semantic failure and permanent HTTP failure do not retry. A successful result resets the
provider/model circuit. Terminal failure returns `parser_unavailable`; the adapter never invents a
meal and never switches to fixture mode.

## Telemetry and rollout gates

The telemetry row contains only provider/model, prompt/schema versions, latency, retry count,
optional token counts, output SHA-256, status, and error code. It deliberately cannot reconstruct
the meal or output.

Hosted mode must remain disabled in production until provider API mapping, contractual privacy,
data residency, retention/training policy, secret management, staging Vietnamese benchmark,
capacity limits, and operational alerts are reviewed. The approved gateway sends `store=false`, but
production hosted parsing still requires the owner-approved provider retention/privacy gate. This
implementation and its controlled tests do not authorize production traffic or production eligibility.
