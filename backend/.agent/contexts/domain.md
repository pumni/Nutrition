Context release: agent-context-2.0.0
Derived from: `crates/domain/src/calculation.rs`, `crates/domain/src/nutrition.rs`, `crates/domain/src/ids.rs`, `docs/FOUNDATION_DECISIONS.md`, `docs/archive/nutrition_backend_blueprint_v1.0/05_NUTRITION_CALCULATION_SPEC.md`, `docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
Use when: changing `crates/domain/**` or calculation-related fixtures and documentation.
Do not infer: new nutrition rules, rounding policy, evidence semantics, or a behavior-version change.

`crates/domain` owns IDs, nutrient codes and units, evidence quality, mass estimates, composition snapshots, resolved item inputs, and `DeterministicCalculator`. Calculation is pure and consumes prepared evidence; it does not call a database, network, provider, clock, or random generator.

Use `rust_decimal::Decimal` for domain arithmetic and preserve missing/zero/trace semantics. Lower and upper mass bounds are propagated into nutrient results. Intermediate values are not rounded. Domain dependencies remain limited to domain-safe crates; provider, HTTP, SQLx, and Tokio dependencies are forbidden.

Canonical gates: `cargo-fmt`, `cargo-clippy`, and `cargo-test`. Named calculator/golden tests and behavior-version decisions are packet acceptance or decision criteria, not additional executable gate definitions.
