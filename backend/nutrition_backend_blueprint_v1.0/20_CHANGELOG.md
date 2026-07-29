# Blueprint changelog

**Phiên bản:** 1.0.0  
**Release date:** 2026-07-23


## 1.0.0 — 2026-07-23

### Status

Upgraded from architecture baseline/v0.8 to development-ready blueprint v1.0.

### Fixed

- Filled previously empty `02_PRODUCT_SCOPE_AND_REQUIREMENTS.md`.
- Filled previously empty `12_ARCHITECTURE_DECISION_RECORDS.md`.
- Removed ambiguity between “baseline” and “development-ready”.
- Added explicit distinction between target architecture and delivery sequence.

### Added

- Requirement IDs, use cases, NFR/SLO, acceptance and non-goals.
- Walking skeleton Phase 0.5.
- Architecture fitness functions.
- ADR context/consequences/reconsider triggers.
- Open-source/market build-buy-reuse strategy.
- External provider baseline/shadow/anti-corruption strategy.
- VietnameseMealBench end-to-end specification.
- Clarification/correction state machine and UX contract.
- Dataset/provider source adapter contract.
- Release, behavior, data and change-management policy.
- Version metadata and cross-document links.

### Strengthened

- Product feedback loop is now first-class.
- Unknown/ambiguity handling and insufficient-evidence behavior.
- Source release staging/activation/rollback.
- Reproducibility version vector.
- Evaluation slices and regression gates.
- Security/privacy around provider payloads and product analytics.
- Curation capacity and operational gates.

### Deferred explicitly

- Vector search, LLM reranker, Redis, Kafka, Kubernetes.
- Full ontology import.
- Image input and personalization.
- User-facing probability confidence before calibration.
- Full micronutrients before source-quality maturity.

### Compatibility

Core domain decisions from baseline remain compatible:

- unified `food_entity`;
- recipe/version split;
- composition profiles as evidence;
- contextual portion observations;
- deterministic calculator;
- immutable analysis revisions;
- PostgreSQL-first modular monolith.
