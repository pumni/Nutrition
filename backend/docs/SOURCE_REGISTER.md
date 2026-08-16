# Source register

This register blocks production ingestion until license, provenance, artifact, selection, and
validation review are complete. Source approval for v1 does not by itself authorize catalog
activation.

| Source code | Purpose | License / rights status | Production activation | Owner | Next evidence |
|---|---|---|---|---|---|
| `foundation_fixture` | Deterministic food, portion, calculation, and HTTP tests | Internal test-only data | Prohibited | Engineering | Replace with reviewed source-backed release |
| `usda_fdc` | Basic food composition; initial target is Foundation Foods April 2026 JSON | `approved_for_v1`: USDA FDC public domain / CC0 1.0 | Prohibited until the reviewed allowlist, validation report, named reviewer approval, and explicit activation gates pass | `pumni` (data/domain reviewer) | Review `fdc-foundation-2026-04-selection-candidate-v1.json`; record the exact allowlist hash and approval reference; the release-specific null-tail adapter is recorded in `releases/fdc-foundation-2026-04-validation.md` |
| `vn_food_composition_2017` | Candidate Vietnamese composition evidence | `not_selected_for_v1`; rights review deferred; FAO/INFOODS listing is not a reuse grant | Prohibited | Deferred | Do not ingest or redistribute for v1; reopen rights review only if a later release selects this source |
| `vn_curated_seed` | Curated Vietnamese identities, aliases, and future recipe evidence | Project-curated records require explicit source-level provenance for every derived value | Prohibited until curation/recipe policy is approved | Unassigned domain owner | Define curation and recipe evidence protocol; do not synthesize nutrient profiles with an LLM |
| `portion_measurement` | Project-controlled household/count/volume portion observations | Future project-generated measurement evidence | Prohibited until measurement protocol and review are complete | Unassigned domain owner | Complete #7 and publish a reviewed staged measurement release |

## Initial production strategy

The source strategy is defined in [`PRODUCTION_DATA_STRATEGY.md`](PRODUCTION_DATA_STRATEGY.md).
The first composition importer target is the release-pinned USDA FoodData Central Foundation Foods
April 2026 JSON artifact. Runtime analysis must never depend on mutable current FDC API results.

The 2017 Vietnamese national table is explicitly excluded from v1. A Vietnamese alias can map to
a reviewed food identity without changing the provenance of its composition profile.

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

For the first FDC release, `pumni` is the named data/domain reviewer. Technical eligibility is
only candidate evidence: it must not be recorded as `approved` without the human review artifact.

Source data is imported into a staged catalog release. Validation precedes explicit activation, and
an active release is never edited in place. Activation requires the validation report hash to match
the staged release manifest, complete energy/provenance evidence, approved source mappings, and an
explicit reviewer approval reference; the previous source and catalog releases remain available
for rollback. Rollback creates a new staged snapshot rather than mutating a superseded release.

The two foods and two contextual portion observations in the foundation fixture are synthetic
engineering data. Their nutrient values and mass ranges must not be represented as a curated
production catalog release.
