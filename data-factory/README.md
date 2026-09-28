# Nutrition Data Factory

This repository is the isolated implementation of the Nutrition Data Factory handoff and the
`DATA-100..160` release-candidate work. It is deterministic tooling and evidence boundaries; it
does not acquire sources over the network, infer food facts, or connect to the backend database.

## Scope

The scaffold provides boundaries for:

- source metadata and registry lookup;
- explicit `approved`, `reference_only`, `pending_unknown`, `prohibited`, and `test_only` rights states;
- fail-closed production-candidate rights validation;
- content-addressed artifact storage using SHA-256;
- acquisition metadata, expected checksum/size checks, schema fingerprints, and source-record hashes;
- offline release-pinned FDC Foundation adapter and explicit core nutrient crosswalk;
- full approved-release FDC normalization with raw-record/nutrient preservation and explicit
  quarantine accounting;
- versioned source nutrient registry that preserves unmapped FDC nutrient semantics for review;
- non-mutating quality reports and read-only backend 20-record compatibility evidence;
- a synthetic adapter and deterministic normalization;
- curation proposals that cannot approve themselves;
- Vietnamese phrase candidate extraction with private-context omission by default;
- human-only curation decisions with non-destructive supersession;
- release impact reports for changes, provenance deltas, coverage, and fixed cases;
- versioned portion-study manifests, instrument/tare/sample validation, deterministic estimates, and immutable review packets;
- backend-compatible full release package containing raw, catalog, composition, recipe and
  portion boundaries plus a completeness manifest;
- canonical `catalog-handoff-v1/` package for the reviewed 20-record backend handoff.
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

## Full approved-source catalog candidate

The 20 IDs in `config/backend-fdc-selection.json` are retained only as a regression/compatibility
slice. They are not the full catalog universe. To compile every accepted record from the pinned
FDC Foundation release, use the full-source runner:

```powershell
python scripts/run_fdc_full_release.py `
  --archive artifacts/raw/usda_fdc_foundation/2026-04-30/FoodData_Central_foundation_food_json_2026-04-30.zip `
  --extracted artifacts/raw/usda_fdc_foundation/2026-04-30/extracted/FoodData_Central_foundation_food_json_2026-04-30.json `
  --object-store artifacts/objects `
  --output artifacts/derived/usda_fdc_foundation/2026-04-30/full-release-v22 `
  --backend-baseline 479ac773b372599e2648437bfe5b56620f1b706d `
  --retrieved-at 2026-08-20T00:00:00+00:00
```

When an evidence unblock pack is available, add its inputs to the same run:

```powershell
  --vietnamese-corpus <pack>/01_vietnamese_corpus_seed.json `
  --recipe-evidence <pack>/02_recipe_evidence_candidates.json `
  --portion-evidence <pack>/03_portion_source_candidates.json `
  --portion-plan <pack>/04_physical_portion_measurement_plan.md
```

Vietnamese phrases become proposal-only review packets. Semantic source mapping proposals are emitted
only when all identity/preparation/specificity constraints match; no compatible FDC mapping is emitted
for the current Vietnamese corpus in the v22 evidence snapshot. Institutional recipe and portion sources
become evidence candidates with hashes and missing-evidence reports; they do not become approved
recipes, calculated profiles or published portions automatically.

Recipe evidence reports also emit per-ingredient semantic review, non-mass quantity states and a
deterministic priority queue. Unknown ingredients fail closed; only exact or constraint-compatible
source records become review proposals, and no proposal compiles a recipe automatically. Portion
evidence preserves institutionally reported weights as source evidence, records edible-basis gaps,
and emits a ranked physical-measurement queue; household-unit gram conversion is never inferred.

The runner verifies the pinned archive and extracted JSON hashes, accounts for every raw source
row, preserves raw nutrient observations, and writes both `full-catalog-package/` and
`catalog-handoff-v1/`. The handoff package is exactly the nine importable core files, uses contract
`catalog-handoff-1.0.0` and profile `fdc-foundation-reviewed-selection-v1`, exports only the four
backend-supported nutrients, and is always `staged_only` with both production flags false. Source-quality errors,
forbidden/negative nutrient observations, unmapped source nutrient IDs, absent Vietnamese corpus,
absent recipe evidence and absent measured portions remain explicit in the reports; a successful
command does not mean the nutrition database is complete or published.

The committed cross-component golden fixture is deliberately separate: it uses profile
`catalog-handoff-test-fixture-v1`, source `synthetic_fixture`, and test-only rights. It is useful for
offline producer/consumer/PostgreSQL contract tests, but it is not an FDC release and cannot stand
in for the caller-supplied USDA artifacts.

The current reproducible evidence snapshot is committed at
`docs/releases/data-coverage-fdc-foundation-2026-04.json`. Raw and derived artifacts are intentionally
kept outside Git and referenced by SHA-256.

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

Build the human release-gate packet after a candidate is created:

```powershell
python scripts/build_release_review_packet.py `
  --run-dir artifacts/derived/usda_fdc_foundation/2026-04-30/dry-run-v4 `
  --package-dir artifacts/derived/usda_fdc_foundation/2026-04-30/dry-run-v4/candidate-package `
  --backend-head <read-only-backend-commit> `
  --output artifacts/derived/usda_fdc_foundation/2026-04-30/dry-run-v4/release-gate-report.json
```

The packet distinguishes technical `passed` checks from `review_required` decisions. It cannot
turn machine curation proposals, source anomalies, impact gaps or missing rollback approval into
an activation decision.

## Human portion measurement packet

Issue #36 tooling uses a versioned manifest and measurement document:

- `schemas/portion-study-manifest-0.2.0.json` records the reviewed target, preapproved sample plan,
  instrument resolution/calibration check, tare method/mass, operators, and estimator policies.
- `schemas/portion-measurements-0.2.0.json` records each positive net mass, batch, timestamp,
  target context, whether the row is an independent sample or a repeat linked to its primary
  weighing, and required sample-accounting metadata.
- `scripts/prepare_portion_review_packet.py` validates supplied files, stores their exact bytes in a
  local SHA-256 content-addressed artifact store, and emits a create-only deterministic review
  packet. It does not access PostgreSQL, stage a catalog release, or activate evidence.

Manifest schema `0.2.0` is a narrow successor to `0.1.0`: it adds explicit human review states for
identity/preparation/context, an approved sample/batch plan, and numeric calibration/tare checks.
The measurement protocol remains `portion-measurement-0.1.0`; this schema change does not relax
identity matching or enable publication.

Measurement document schema `0.2.0` requires the collected sample ID inventory, an explicit
`all_samples_accounted_for` state, and a structured `deviations_or_exclusions` list (`[]` means none).
Each deviation or exclusion names its sample, reason, and audit reference. Every collected ID must
have an independent observation or an explained exclusion; incomplete accounting blocks estimates
and review readiness. Review packet `portion-review-packet-0.2.0` retains this block alongside the
hash-pinned raw document.

The estimator averages the repeat readings within each independent sample, computes the central
estimate as the equal-weight mean of independent-sample means, and uses their minimum and maximum
as lower and upper bounds. The approved study plan supplies the minimum sample and batch counts;
the tool does not choose those thresholds. An unresolved food identity, preparation, or physical
measure context suppresses the estimate. Packets remain publication-blocked even after technical
validation.

Do not create a target manifest or collect/publish Vietnamese measurements until #35/#41 has reviewed
the identity and source strategy. Current #36 scope contains no selected targets or measurement
observations. The issue handoff records this blocked state at
`docs/reviews/vietnamese-portion-measurement-issue-36.md`.

After those decisions and actual measurements are available, run the packet tool outside the
repository. Omit `--measurements` until a genuine measurement file exists:

```powershell
python scripts/prepare_portion_review_packet.py `
  --manifest C:\secure\portion-study\study-manifest.json `
  --artifact-store C:\secure\portion-study\artifacts `
  --output C:\secure\portion-study\review-packet.json
```

Add `--measurements C:\secure\portion-study\measurements.json` only when it contains recorded
human measurements. Supply `--reviewer` and `--review-ref` only for an existing named human review.

## Ready-task verification

```text
python scripts/test.py
python -m compileall -q src tests scripts
```

The tests use only local synthetic fixtures. The full-source command uses caller-supplied, hash-pinned
artifacts. Vietnamese identity candidates remain proposals until human approval; recipes require
source-backed ingredient identities and yield evidence; portions require measured study manifests and
never infer grams from household-unit words. Backend integration and production activation are not
performed by this repository.

## Safety boundary

This repository does not contain backend credentials, database connections, or production activation
code. FDC artifacts remain caller-supplied evidence, and product-facing mappings still require human
review before any backend integration.
