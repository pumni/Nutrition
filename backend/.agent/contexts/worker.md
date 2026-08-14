Context release: agent-context-2.0.0
Derived from: `crates/worker/src/main.rs`, `crates/persistence-postgres/src/ops_repository.rs`, `docs/FOUNDATION_DECISIONS.md`, `docs/SECURITY_AND_OPERATIONS.md`
Use when: changing `crates/worker/**` or explicitly included jobs/outbox persistence.
Do not infer: unbounded retries, new transports, lease semantics, arbitrary payload execution, or a new worker mode.

The worker supports `WORKER_MODE=idle`, `run-once`, and `loop`. Jobs use `FOR UPDATE SKIP LOCKED`, bounded attempts, lease ownership/timestamps, and terminal `dead` state. Unsupported job types fail closed. The current outbox run-once path targets a database-local verification sink and marks delivery with `published_at`; external transport remains deferred.

Graceful shutdown and bounded batches are part of the worker reliability boundary. External side effects require idempotency and explicit human scope.

Canonical gates: `cargo-fmt`, `cargo-clippy`, and `cargo-test`. Worker mode/lease/outbox tests run under the canonical test gate; add `postgres-verify` when database-backed job semantics change.
