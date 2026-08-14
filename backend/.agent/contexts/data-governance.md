Context release: agent-context-2.0.0
Derived from: `fixtures/vietnamese-meal-bench/manifest.json`, `fixtures/vietnamese-meal-bench/foundation-cases.json`, `seeds/0001_foundation_fixture.sql`, `docs/FOUNDATION_DECISIONS.md`, `docs/RISK_REGISTER.md`, `docs/archive/nutrition_backend_blueprint_v1.0/07_DATA_SOURCE_QUALITY_AND_GOVERNANCE.md`, `docs/archive/nutrition_backend_blueprint_v1.0/18_SOURCE_ADAPTER_CONTRACT.md`, `docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
Use when: changing `seeds/**`, `fixtures/**`, source/governance documents, or explicitly named catalog release artifacts.
Do not infer: production activation, source licensing, canonical publication, or fixture evidence beyond its declared test-only status.

Source release flow is acquire, verify, parse, validate, map, report, approve, then activate. Raw source artifacts and published release contents are immutable. Human curation is required before canonical mapping, recipe, composition, or portion publication.

The foundation fixture catalog and Vietnamese meal benchmark are test artifacts. Unknown food is not force-matched, and fixture data must not be mistaken for production evidence. Changes to fixtures or seeds require provenance, release, and verification evidence in the packet.

Verification evidence may use canonical atomic IDs such as `cargo-test`, `schema-validation`, `postgres-verify`, and `benchmark-external` when the Task Spec compiler derives those checks.
