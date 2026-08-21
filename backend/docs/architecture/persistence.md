# Persistence architecture

`persistence-postgres` owns catalog/evidence lookup, analysis revisions, snapshots, privacy data
operations, and worker persistence. It may depend inward on `application` and `domain`; it must
not depend on API or worker composition crates.

The persistence boundary preserves transaction-scoped creation/finalization, idempotency key and
request-hash semantics, append-only completed revisions, snapshot SHA-256 verification before
deserialization, and ownership-scoped reads. See [foundation](foundation.md) for the current
vertical slice and the `crates/persistence-postgres/src/` modules for implementation details.

Run `cargo xtask migrations` after migration-related changes and `cargo xtask postgres` when the
database boundary is exercised.
