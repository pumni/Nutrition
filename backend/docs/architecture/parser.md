# Hosted parser contract

Status: foundation transport contract  
Behavior release: `foundation-0.6.0`

## Purpose

The hosted model is a constrained language parser only. Nutrition values, food resolution,
portion mass, composition selection, and calculation remain deterministic backend responsibilities.
The model must not return calories, nutrients, internal IDs, URLs, or inferred gram weights.

The adapter contract is provider-neutral. The approved v1 gateway maps it to the OpenAI Responses
API at `https://api.openai.com/v1/responses` using provider `openai` and model `gpt-5.6-luna`.
The gateway must not fall back to another provider or model.

## Request envelope

The adapter sends an HTTPS bearer-authenticated JSON request containing:

- provider and exact model identifiers;
- a fixed system instruction;
- the exact `parsed-meal-0.1.0` JSON Schema;
- a repair flag that is true only for the single schema-repair retry;
- `input.locale` and `input.untrusted_meal_text`.

No user ID, authorization header value, account metadata, meal history, resolved food ID,
nutrition result, or source URL is placed in the JSON body. The bearer secret exists only in the
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

Unknown envelope fields are rejected. Token fields are optional but cannot be negative. The
response is streamed into a buffer with a configurable hard limit; declared and actual oversized
responses fail closed.

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
