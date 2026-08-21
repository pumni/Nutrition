# Architecture index

Use this directory for subsystem structure and dependency boundaries.

- [Foundation](foundation.md) — deterministic calculation, evidence, revisions, and release model
- [Hosted parser](parser.md) — language-only provider boundary and fail-closed validation
- [Persistence](persistence.md) — PostgreSQL repositories, transactions, snapshots, and replay
- [HTTP API](api.md) — composition, authentication, ownership, and routes
- [Worker](worker.md) — startup gates, worker modes, leases, and retries

The dependency direction is enforced by `cargo xtask architecture`:

```text
domain <- application <- adapters
                      <- persistence-postgres
                      <- api-http / worker (composition edges)
```
