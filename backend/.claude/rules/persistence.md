---
paths:
  - "crates/persistence-postgres/**"
  - "migrations/**"
---

# Persistence rules

- Existing recorded migrations are immutable; database changes use new forward migrations.
- Preserve transaction boundaries, replay hashes, and immutable completed revisions.
- Released catalog evidence remains immutable and versioned.
- Run `cargo xtask migrations`, `cargo xtask postgres`, and the relevant targeted tests for
  persistence changes.

Authoritative details: [docs/architecture/foundation.md](../../docs/architecture/foundation.md) and
[docs/operations/configuration.md](../../docs/operations/configuration.md).
