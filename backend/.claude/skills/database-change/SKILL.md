---
name: database-change
description: Apply and verify a database or migration change while preserving forward-only and immutable evidence rules.
---

# Database change

Read [docs/architecture/foundation.md](../../../docs/architecture/foundation.md) and the persistence
rule before editing. Preserve existing migration files; add a forward migration when needed.
Run targeted persistence tests, then `cargo xtask postgres` and the normal verification path when
the environment permits. Do not activate production data or change release policy.
