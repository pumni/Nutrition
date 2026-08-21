---
paths:
  - "crates/api-http/**"
---

# API security rules

- Preserve ownership and authentication boundaries; reads and writes remain owner-scoped.
- Never log tokens, claims, authorization material, meal content, or provider responses.
- Development authentication remains local/CI behavior and must not leak into production modes.
- Public API changes require contract tests and an update to the current API documentation.

Authoritative details: [docs/operations/security.md](../../docs/operations/security.md) and
[docs/product/api-v1.md](../../docs/product/api-v1.md).
