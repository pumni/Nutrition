---
paths:
  - "crates/adapters/src/hosted_parser/**"
  - "schemas/*parser*"
---

# Hosted parser rules

- The LLM parses language structure only; it does not supply nutrition facts, canonical IDs, gram
  weights, or calories.
- Treat provider output as untrusted. Preserve schema validation, semantic validation, retry, and
  circuit-breaker behavior.
- Never log raw meal text or raw provider responses.
- Hosted failures must not fall back to fixture parsing.

Authoritative details: [docs/architecture/parser.md](../../docs/architecture/parser.md).
