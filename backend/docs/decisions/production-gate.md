# ADR: Production activation gate

**Status:** Accepted implementation/staging boundary; production activation is not authorized.

## Decision

Production requires the provider privacy gate, approved and passed Vietnamese benchmark threshold,
an explicitly production-eligible catalog package with human evidence, reviewed staging
SLO/load/restore evidence, and a reviewed release/rollback manifest. The owner remains the sole
authority for production traffic, provider enablement, catalog activation, release tags, and
canonical publication.

## Evidence / affected paths

- `docs/operations/staging-release-gate.md`
- `docs/releases/release-evidence-candidate.md`
- `docs/evidence/vietnamese-meal-bench.md`
