# Catalog handoff contract v1

Contract version: `catalog-handoff-1.0.0`
Package kind: `nutrition-catalog-handoff`
Initial profile: `fdc-foundation-reviewed-selection-v1`

This directory is the only canonical schema source for the Data Factory → Backend catalog
handoff. The schemas describe one JSON object per JSONL line; they do not describe a JSONL file as
an array.

The initial profile is intentionally narrow:

- USDA FDC Foundation release `2026-04-30`;
- canonical source code `usda_fdc_foundation` (distinct from the legacy raw importer code);
- the reviewed 20-record selection in `data-factory/config/backend-fdc-selection.json`;
- exact source identity only;
- `energy_kcal`, `protein_g`, `fat_g`, and `carbohydrate_g` only;
- source-backed English names and four grounded core composition values per selected record.

Every generated package contains seven manifest-declared payload files plus `manifest.json` and
`checksums.sha256`. The checksum file hashes the manifest and payload files, but never itself. JSON
is UTF-8 with sorted keys, LF line endings, and exactly one final newline. JSONL records are sorted
by stable domain keys and use compact deterministic JSON.

The package is always `staged_only`, `production_eligible: false`, and
`activation_authorized: false`. Those values are safety invariants, not configuration defaults.

Record identity is source-derived. FDC food entities use the backend semantic key
`usda-fdc:<fdc_id>`; no random UUID or inferred food, portion, recipe, alias, or nutrient mapping
is created by this contract.

The committed `fixtures/minimal-valid` directory is the cross-component golden fixture. It is
synthetic test data only and must not be activated or treated as production evidence.
