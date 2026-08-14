Context release: agent-context-1.0.0
Derived from: `crates/api-http/src/main.rs`, `docs/FOUNDATION_DECISIONS.md`, `docs/SECURITY_AND_OPERATIONS.md`, `docs/archive/nutrition_backend_blueprint_v1.0/08_API_AND_RUNTIME_ARCHITECTURE.md`, `docs/archive/nutrition_backend_blueprint_v1.0/10_SECURITY_PRIVACY_AND_PRODUCT_SAFETY.md`, `docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
Use when: changing `crates/api-http/**`.
Do not infer: production authentication, broadened ownership, new endpoints, larger request limits, idempotency changes, or sensitive logging.

The API composes Axum routes with application services, adapters, and PostgreSQL repositories. The foundation supports development bearer authentication only; non-development startup is blocked until a reviewed OIDC adapter exists. Analysis reads and writes enforce ownership, missing credentials return `401`, and another owner's resource returns `403`.

Request bodies are capped at 16 KiB. Create and correction idempotency keys are scoped as defined by the foundation contract. API and tracing logs exclude meal text, authorization headers, raw text, and database URLs.

Canonical gates: `cargo-fmt`, `cargo-clippy`, and `cargo-test`. API smoke/contract tests are covered by the applicable canonical test gate; add `postgres-verify` when ownership or persistence routes change.
