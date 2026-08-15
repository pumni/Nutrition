# Source register

This register blocks production ingestion until license, provenance, artifact, and validation review
are complete. Source approval for importer development does not by itself authorize catalog
activation.

| Source code | Purpose | License / rights status | Production activation | Owner | Next evidence |
|---|---|---|---|---|---|
| `foundation_fixture` | Deterministic food, portion, calculation, and HTTP tests | Internal test-only data | Prohibited | Engineering | Replace with reviewed source-backed release |
| `usda_fdc` | Basic food composition; initial target is Foundation Foods April 2026 JSON | USDA FDC reviewed 2026-08-15: public domain / CC0 1.0 | Prohibited until release artifact, importer, validation, reviewer, and rollback gates pass | Unassigned data owner | Approve a reviewed subset, assign a named reviewer, and complete #6; the release-specific null-tail adapter is recorded in `releases/fdc-foundation-2026-04-validation.md` |
| `vn_food_composition_2017` | Candidate Vietnamese composition evidence | Ministry of Health publication listed by FAO/INFOODS as print-only; reuse/redistribution rights not established | Prohibited | Unassigned data/domain owner | Obtain and record permitted-use and redistribution terms before importer work |
| `vn_curated_seed` | Curated Vietnamese identities, aliases, and future recipe evidence | Project-curated records require explicit source-level provenance for every derived value | Prohibited until curation/recipe policy is approved | Unassigned domain owner | Define curation and recipe evidence protocol; do not synthesize nutrient profiles with an LLM |
| `portion_measurement` | Project-controlled household/count/volume portion observations | Future project-generated measurement evidence | Prohibited until measurement protocol and review are complete | Unassigned domain owner | Complete #7 and publish a reviewed staged measurement release |

## Initial production strategy

The source strategy is defined in [`PRODUCTION_DATA_STRATEGY.md`](PRODUCTION_DATA_STRATEGY.md).
The first composition importer target is the release-pinned USDA FoodData Central Foundation Foods
April 2026 JSON artifact. Runtime analysis must never depend on mutable current FDC API results.

Vietnamese national-table ingestion remains blocked until permitted-use and redistribution evidence
is recorded. A Vietnamese alias can map to a reviewed food identity without changing the provenance
of its composition profile.

Portion measurements remain independent from composition evidence. Unsupported portions clarify or
fail; they are not inferred by the hosted parser.

## Activation rule

A source cannot become active until it has:

- publisher and owner;
- license/terms snapshot and permitted-use review;
- release identifier and artifact SHA-256;
- schema fingerprint;
- importer version;
- validation and impact report;
- reviewer and rollback target.

Source data is imported into a staged catalog release. Validation precedes explicit activation, and
an active release is never edited in place. Activation requires the validation report hash to match
the staged release manifest, complete energy/provenance evidence, approved source mappings, and an
explicit reviewer approval reference; the previous source and catalog releases remain available
for rollback. Rollback creates a new staged snapshot rather than mutating a superseded release.

The two foods and two contextual portion observations in the foundation fixture are synthetic
engineering data. Their nutrient values and mass ranges must not be represented as a curated
production catalog release.
