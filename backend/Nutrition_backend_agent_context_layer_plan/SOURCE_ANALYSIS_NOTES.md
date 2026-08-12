# Source Analysis Notes

This implementation pack was tailored to the repository baseline rather than generated as a generic AI-coding template.

Repository facts used to make architectural decisions:

- Workspace contains six crates: domain, application, adapters, persistence-postgres, api-http, worker.
- Foundation 0.6.0 implements evidence-first nutrition analysis with deterministic calculation and append-only revision behavior.
- Hosted LLM is a bounded language parser only. It must not supply nutrition values, IDs, URLs or inferred gram weights.
- Hosted provider output is untrusted and is subject to strict envelope, JSON Schema and semantic grounding checks.
- PostgreSQL is the source of truth.
- Published data and completed revisions have immutability requirements.
- Behavior versions are persisted independently for application/parser/prompt/provider/normalization/resolution/portion/composition/clarification/correction/calculator/catalog release.
- Existing repository verification is PowerShell-based and already performs Cargo checks, JSON parsing, sensitive-log scanning and Docker Compose validation.
- Existing risk register explicitly tracks prompt injection, raw meal text leakage, fixture-vs-production confusion, infrastructure expansion, worker idempotency and revision concurrency.
- Blueprint ADRs already establish modular monolith, PostgreSQL-first, contextual portions, deterministic calculator, LLM extraction-only, append-only revisions, human curation and VietnameseMealBench release gating.

The ACL therefore deliberately does not create a second architecture. It indexes, compresses and enforces the one the project already has.
