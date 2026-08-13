# LLM Boundary Invariants

- The hosted model is a constrained language parser only. Nutrition values, food resolution, portion mass, composition selection, and calculation remain deterministic backend responsibilities.
- The model must not return calories, nutrients, internal IDs, URLs, or inferred gram weights.
- Hosted requests use a configured HTTPS endpoint, bounded configuration, a fixed system instruction, the strict versioned schema, locale, and untrusted meal text. They exclude identity, authorization, history, nutrition data, internal IDs, and source URLs.
- Responses are bounded and validated in order: strict envelope, strict versioned JSON Schema, typed deserialization, source-span and food-phrase grounding, negation and duplicate checks, then deterministic unit normalization.
- Only one retry is allowed for the specified transient or schema-invalid cases. Semantic and permanent failures do not retry. Terminal failure is `parser_unavailable`; hosted parsing never silently falls back to fixture mode.
- Hosted telemetry contains only bounded operational metadata and output SHA-256; it cannot reconstruct the meal or response.

Sources:

- `docs/FOUNDATION_DECISIONS.md`
- `docs/HOSTED_PARSER.md`
- `docs/RISK_REGISTER.md`
- `nutrition_backend_blueprint_v1.0/00_README.md`
- `nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
