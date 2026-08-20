# Nutrition Data Factory

This repository is the isolated implementation of the ready Nutrition Data Factory handoff tasks
`DATA-000`, `DATA-001`, `DATA-002`, `DATA-010`, `DATA-011`, `DATA-012`, `DATA-013`, `DATA-020`,
`DATA-021`, `DATA-030`, `DATA-031`, and `DATA-040`. It is deterministic tooling and evidence
boundaries; it does not acquire sources over the network, infer food facts, or connect to
`Nutrition_backend`.

## Scope

The scaffold provides boundaries for:

- source metadata and registry lookup;
- explicit `approved`, `reference_only`, `pending_unknown`, `prohibited`, and `test_only` rights states;
- fail-closed production-candidate rights validation;
- content-addressed artifact storage using SHA-256;
- acquisition metadata, expected checksum/size checks, schema fingerprints, and source-record hashes;
- offline release-pinned FDC Foundation adapter and explicit core nutrient crosswalk;
- non-mutating quality reports and read-only backend 20-record compatibility evidence;
- a synthetic adapter and deterministic normalization;
- curation proposals that cannot approve themselves;
- Vietnamese phrase candidate extraction with private-context omission by default;
- human-only curation decisions with non-destructive supersession;
- release impact reports for changes, provenance deltas, coverage, and fixed cases;
- portion-study validation/compiler with independent-sample and repeat-weighing semantics.
- non-mutating validation and release-package compilation.

The seed registry preserves the handoff policy: USDA FDC Foundation is an approved candidate after
project gates; FNDDS, FAO/INFOODS, SMILING Vietnam and ASEAN remain reference-only; the 2017 Vietnam
table remains prohibited. Downloadable or free-access status never upgrades a rights state.

The fixture contains no nutrition claims. It exists only to prove the shape
`ingest -> normalize -> validate -> package`.

## Requirements

- Python 3.11 or newer;
- standard library only;
- no network access is required by tests.

## Test

```text
python scripts/test.py
```

The command runs the unit suite with an isolated temporary directory. Tests use only the synthetic
fixture and do not call external services.

## Synthetic pipeline

```text
python -m nutrition_data_factory pipeline \
  --config config/example.json \
  --fixture tests/fixtures/synthetic-source.json \
  --output .tmp/synthetic-release
```

On PowerShell, set `PYTHONPATH=src` before invoking the module:

```powershell
$env:PYTHONPATH = "src"
python -m nutrition_data_factory pipeline --config config/example.json --fixture tests/fixtures/synthetic-source.json --output .tmp/synthetic-release
```

The output is checksum-bound and has `production_eligible: false`. Re-running with the same inputs in
a fresh output directory produces byte-equivalent package files.

## Offline FDC artifact dry-run

The pinned FDC adapter accepts caller-supplied April 2026 artifacts and never downloads at runtime.
After acquiring the exact archive and extracted JSON separately, run:

```powershell
python scripts/run_fdc_dry_run.py `
  --archive artifacts/raw/usda_fdc_foundation/2026-04-30/FoodData_Central_foundation_food_json_2026-04-30.zip `
  --extracted artifacts/raw/usda_fdc_foundation/2026-04-30/extracted/FoodData_Central_foundation_food_json_2026-04-30.json `
  --object-store artifacts/objects `
  --output artifacts/derived/usda_fdc_foundation/2026-04-30/dry-run
```

The run records acquisition, parse, full-source quality, selected-candidate quality, compatibility,
crosswalk and validation reports. The candidate quality gate is scoped to the reviewed backend
selection; full-source anomalies remain visible in `source-quality-report.json`. It creates a
candidate package only when every hard gate for that selection passes, and it never activates
production.

## Ready-task verification

```text
python scripts/test.py
python -m compileall -q src tests scripts
```

The tests use only local synthetic fixtures. Blocked handoff tasks (`DATA-100`, `DATA-110`, `DATA-120`,
and `DATA-130`) remain unimplemented by policy. Backend integration and production activation are not
performed by this repository.

## Safety boundary

This repository does not contain backend credentials, database connections, or production activation
code. FDC artifacts remain caller-supplied evidence, and product-facing mappings still require human
review before any backend integration.
