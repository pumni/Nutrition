# Architecture Invariants

- The foundation is a modular monolith: one codebase, API and worker process types, and one PostgreSQL database with logical schemas.
- PostgreSQL is the primary source of truth. Redis, Kafka, vector search, graph databases, Kubernetes, and similar infrastructure are not added without measured need and an ADR.
- Dependency direction is `domain <- application <- adapters`, persistence, API, and worker. The domain does not import Axum, SQLx, Tokio, provider SDKs, clocks, or random generators.
- Nutrition calculation is a pure deterministic domain operation with no network, database, or LLM calls.
- External providers cross an anti-corruption adapter and do not write provider fields directly into domain or canonical database models.
- Canonical publication requires human curation; automated ingestion or candidate generation does not publish canonical data by itself.

Sources:

- `docs/FOUNDATION_DECISIONS.md`
- `docs/RISK_REGISTER.md`
- `docs/FOUNDATION_DECISIONS.md`
