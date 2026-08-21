# ADR: Initial Vietnamese catalog scope

**Status:** Accepted for staged implementation; production eligibility is separately gated.

## Context

Vietnamese identity and portion evidence must be reviewed, versioned, and provenance-bound. Tooling
or model output alone cannot establish production evidence.

## Decision

Use the versioned VietnameseMealBench-derived corpus and reviewed evidence packages, with the
approved exact FDC Foundation allowlist as the initial activatable composition slice. Exact
identities, approved aliases, reviewed recipe evidence, contextual portions, and source provenance
are required. Fuzzy resolution, hidden household conversions, prohibited sources, and AI-generated
production evidence are not allowed.

## Consequences

FDC imports remain staged and activation remains explicit. Unknown or unsupported identity/portion
cases clarify or remain insufficient rather than being guessed.

## Evidence / affected paths

- `docs/evidence/nutrition-sources.md`
- `docs/evidence/portions.md`
- `docs/evidence/vietnamese-catalog/`
- `scripts/validate-vietnamese-catalog-evidence.ps1`
