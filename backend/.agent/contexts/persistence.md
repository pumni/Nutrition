Context release: agent-context-2.0.0
Derived from: `crates/persistence-postgres/src/analysis_repository.rs`, `crates/persistence-postgres/src/catalog_repository.rs`, `crates/persistence-postgres/src/portion_repository.rs`, `crates/persistence-postgres/src/parser_telemetry.rs`, `crates/persistence-postgres/src/ops_repository.rs`, `migrations`, `docs/FOUNDATION_DECISIONS.md`
Use when: changing `crates/persistence-postgres/**` or `migrations/**`.
Do not infer: schema changes, migration edits, transaction boundaries, weaker immutability, or unversioned replay dependencies.

The PostgreSQL adapter owns catalog and portion evidence lookup, analysis/revision/item/result/snapshot persistence, parser telemetry, jobs, outbox state, and seed support. The analysis workflow writes its relational rows, immutable snapshot, hash, and outbox event in one transaction. Completed revisions and published catalog data are protected from mutation.

Idempotency keys are stored with request hashes and immutable response revisions. Applied migrations are not edited; schema changes are forward migrations and require the database verification gate.

Canonical gates: `cargo-fmt`, `cargo-clippy`, and `cargo-test`; add `postgres-verify` for database or migration changes.
