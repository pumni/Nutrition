Context release: agent-context-2.0.0
Derived from: `crates/worker/src/main.rs`, `crates/persistence-postgres/src/ops_repository.rs`, `docs/FOUNDATION_DECISIONS.md`, `docs/SECURITY_AND_OPERATIONS.md`, `docs/archive/nutrition_backend_blueprint_v1.0/08_API_AND_RUNTIME_ARCHITECTURE.md`, `docs/archive/nutrition_backend_blueprint_v1.0/09_TESTING_EVALUATION_AND_OBSERVABILITY.md`, `docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
Use when: changing `crates/worker/**` or explicitly included jobs/outbox persistence.
Do not infer: unbounded retries, new transports, lease semantics, arbitrary payload execution, or a new worker mode.

The worker supports `WORKER_MODE=idle`, `run-once`, and `loop`. Jobs use `FOR UPDATE SKIP LOCKED`, bounded attempts, lease ownership/timestamps, and terminal `dead` state. Unsupported job types fail closed. The current outbox run-once path targets a database-local verification sink and marks delivery with `published_at`; external transport remains deferred.

Graceful shutdown and bounded batches are part of the worker reliability boundary. External side effects require idempotency and explicit packet scope.

Canonical gates: `cargo-fmt`, `cargo-clippy`, and `cargo-test`. Worker mode/lease/outbox tests run under the canonical test gate; add `postgres-verify` when database-backed job semantics change.
