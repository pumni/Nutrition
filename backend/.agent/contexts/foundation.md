Context release: agent-context-1.0.0
Derived from: `docs/FOUNDATION_DECISIONS.md`, `docs/archive/nutrition_backend_blueprint_v1.0/00_README.md`, `docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
Use when: reviewing the foundation vertical slice or cross-cutting behavior.
Do not infer: production catalog evidence, new provider behavior, new infrastructure, or unrecorded current configuration.

The current slice is explicit parser selection (`PARSER_MODE=fixture|hosted`), food and portion resolution, deterministic calculation, PostgreSQL persistence, immutable snapshots, SHA-256 replay verification, and append-only analysis revisions. The behavior release is `foundation-0.6.0`.

The fixture parser is a bounded local/test adapter. Hosted parsing is provider-neutral, fail-closed, and never silently falls back to fixture mode. Food identity, exact-name retrieval, portion evidence, composition selection, release pinning, and analysis persistence use PostgreSQL. The calculator uses `rust_decimal::Decimal`, does not round intermediate values, and has no network, database, or LLM dependency.

Important gates include formatting, Clippy, workspace tests, JSON and sensitive-log checks, Docker Compose validation, and PostgreSQL verification when database-backed behavior is changed.
