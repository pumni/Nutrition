Context release: agent-context-1.0.0
Derived from: `crates/application/src/analyze.rs`, `crates/application/src/revise.rs`, `crates/application/src/model.rs`, `crates/application/src/ports.rs`, `docs/FOUNDATION_DECISIONS.md`, `nutrition_backend_blueprint_v1.0/17_CLARIFICATION_CORRECTION_UX_SPEC.md`, `nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
Use when: changing `crates/application/**` analysis, clarification, correction, or ports.
Do not infer: a new orchestration order, public request contract, clarification policy, or revision mutation rule.

`MealAnalysisService` coordinates parsing, food evidence, portion evidence, calculation, and repository persistence through application ports. `AnalysisRevisionService` handles one-question clarification and portion corrections. Models retain snapshots, evidence, status, idempotency context, and the behavior-version vector.

Food and portion evidence are separate ports. Explicit grams need no portion observation; other units require food-specific evidence. Completed revisions are append-only. Idempotency keys and request hashes replay the same immutable revision or return a conflict for a different body.

Required gates: `cargo-fmt`, `cargo-clippy`, `cargo-test`, and the workflow/state tests named by the packet.
