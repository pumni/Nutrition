Context release: agent-context-1.0.0
Derived from: `crates/adapters/src/fixture.rs`, `crates/adapters/src/hosted_parser.rs`, `docs/HOSTED_PARSER.md`, `docs/FOUNDATION_DECISIONS.md`, `docs/RISK_REGISTER.md`, `docs/archive/nutrition_backend_blueprint_v1.0/06_FOOD_RESOLUTION_AND_LLM_SPEC.md`, `docs/archive/nutrition_backend_blueprint_v1.0/16_VIETNAMESE_MEAL_BENCH_SPEC.md`, `docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
Use when: changing the hosted parser adapter, parser schema, or explicitly included parser telemetry.
Do not infer: provider-specific tools, nutrition facts from model output, IDs or gram estimates from the model, raw-text telemetry, redirects, or fixture fallback.

`FixtureParser` is the bounded local/test parser. `HostedMealParser` sends only the bounded HTTPS request envelope and treats provider output as untrusted. Validation is strict envelope, versioned JSON Schema, typed deserialization, grounding, negation/duplicate checks, then deterministic normalization. Responses are byte-bounded.

Only the specified transient or schema retry is allowed. Semantic and permanent failures are terminal `parser_unavailable`; hosted mode does not switch to fixture mode. Telemetry stores operational metadata and output hash only.

Required gates: `cargo-fmt`, `cargo-clippy`, `cargo-test`, hosted parser tests, schema validation, and the benchmark gate when parser behavior, prompt, schema, or provider behavior changes.
