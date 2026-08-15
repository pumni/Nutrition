# Production nutrition data strategy

Status: accepted policy; artifact evidence captured, release validation blocked
Reviewed: 2026-08-15

## Objective

Move the backend from synthetic foundation fixtures to release-pinned production evidence without
weakening the existing deterministic calculation, provenance, immutability, and rollback contracts.

Nutrition composition, food identity, recipes, and portion measurements are different evidence
classes. They must remain independently sourced and versioned even when a product-facing food
concept uses all of them.

## Initial composition source

The first importer target is **USDA FoodData Central Foundation Foods, April 2026 JSON release**.

The reviewed USDA FoodData Central material states that FDC data are public domain and published
under CC0 1.0. USDA publishes dated downloadable releases, and Foundation Foods is specifically
intended for basic, unprocessed or lightly processed foods with analytically derived values and
underlying provenance metadata.

This makes Foundation Foods a better initial fit than a live API dependency:

- the release can be pinned and checksummed;
- the source schema can be fingerprinted;
- imports can be reproduced offline;
- validation can run before catalog activation;
- rollback can target the preceding catalog release;
- runtime nutrition analysis does not change when USDA later publishes new data.

The initial importer should use the downloaded release artifact rather than querying the current
FoodData Central API during a user analysis.

### Initial scope

Include only reviewed Foundation Foods needed for the first production catalog. Do not import all
FoodData Central data types by default.

- **Foundation Foods:** initial composition source.
- **FNDDS:** not part of the first import; reconsider for dietary-survey food concepts or source
  comparisons when a concrete product requirement exists.
- **Branded Foods:** not a canonical source for the initial Vietnamese catalog; product labels and
  manufacturer updates have different evidence semantics and update cadence.
- **SR Legacy:** historical fallback/reference only; do not silently prefer it over newer reviewed
  analytical evidence.

## Vietnamese food evidence

FAO/INFOODS lists a 2017 Vietnamese Food Composition Table from the Ministry of Health, but the
listing describes the resource as print-only. The directory also lists older Vietnamese resources,
including the 2013 SMILING table. Directory availability is not sufficient evidence of permission
to ingest or redistribute those datasets in this product.

Therefore:

- no national Vietnamese composition table is approved for bulk production ingestion yet;
- reuse and redistribution terms must be captured before importer work begins for such a source;
- a Vietnamese alias or display name does not change the provenance of the underlying nutrient
  composition profile;
- a composite Vietnamese dish must not be approximated by an LLM-generated nutrient profile;
- recipe-derived composition requires an explicit recipe-calculation vertical slice and reviewed
  ingredient/yield evidence.

This keeps product delivery unblocked for basic foods while preventing an unclear-rights dataset
from becoming an implicit production dependency.

## Portion evidence

Household, count, and volume portions are a separate evidence class from nutrient composition.

The production policy is:

1. use an explicit source-provided weight only when the source definition and food context match the
   catalog food;
2. otherwise use project-controlled measurements produced under the protocol tracked by #7;
3. store preparation state, measurement context, sample information, and lower/upper mass bounds;
4. never ask the hosted language parser to invent a gram conversion.

A composition release may be activated without every household portion being known. Unsupported
food/unit pairs must continue to clarify or fail with insufficient evidence rather than guess.

## Release and importer contract

Every source import must be represented by immutable release evidence containing at least:

- source code and publisher;
- source release identifier/date;
- artifact filename and SHA-256;
- source schema fingerprint;
- importer version;
- import timestamp;
- record counts and rejection counts;
- nutrient/unit mapping version;
- validation and impact report;
- reviewer;
- rollback catalog release.

The importer writes into a **staged** catalog release. Validation happens before activation.
Activation must be explicit and atomic; an active release is never edited in place.

## Nutrient mapping policy

The importer must map source nutrients to internal nutrient codes explicitly. It must not infer a
mapping from display names at runtime.

For the first product slice, the minimum required nutrients remain:

- `energy_kcal`;
- `protein_g`;
- `carbohydrate_g`;
- `fat_g`.

Source values that use a materially different definition or calculation method must not be merged
under the same internal code merely because their labels look similar. Mapping exceptions belong in
versioned importer policy and the validation report.

### FDC Foundation energy mapping v1

The accepted `fdc_energy_v1` policy maps USDA Foundation Foods energy as follows:

1. prefer nutrient `2048` (Atwater Specific Factor), with method provenance
   `atwater_specific`;
2. fall back to nutrient `2047` (Atwater General Factor), with method provenance
   `atwater_general`;
3. leave the profile incomplete when neither value is valid.

Nutrient `1008` is prohibited for the April 2026 Foundation Foods release and is never a fallback.
Malformed or duplicate `2048` values, invalid units, and other candidate anomalies fail closed rather
than silently selecting a less-specific value. The importer does not derive energy from
macronutrients or from an LLM/provider.

Every imported composition value retains its source nutrient ID and source method, while the profile
retains the source release, importer version, and `fdc_energy_v1` policy identifier. Validation
evidence reports counts for `2048`, `2047`, missing energy, and unexpected `1008` records.

## Food identity policy

External source identifiers are provenance identifiers, not public API identifiers.

- Preserve the source food ID and source release.
- Create internal food identities independently.
- Curated Vietnamese aliases map to reviewed internal foods; they do not modify the imported source
  record.
- Do not fuzzy-match unknown foods into the production catalog during analysis.
- Ambiguous mappings remain staged/rejected until reviewed.

## Source precedence

Do not average or overwrite profiles from different sources automatically. If multiple reviewed
profiles exist for one food concept, composition selection must be an explicit, behavior-versioned
policy. The selected profile and source release remain part of the persisted behavior vector.

## Activation gates

`usda_fdc` may move from candidate to staged-import development after this strategy is accepted, but
production activation remains blocked until:

- the April 2026 Foundation Foods JSON artifact is acquired and SHA-256 recorded;
- importer/schema versions and `fdc_energy_v1` are implemented under #6;
- the selected subset and nutrient mappings pass validation;
- an impact report is reviewed;
- a rollback target is recorded;
- a data/domain owner is assigned.

Vietnamese national-table ingestion remains additionally blocked on documented permitted-use and
redistribution rights.

## Reviewed external evidence

The decision above was based on the official USDA FoodData Central licensing, data-type
and download documentation and the FAO/INFOODS Vietnam food-composition directory, reviewed on
2026-08-15. Those external facts should be captured again in the release evidence when a concrete
artifact is approved so a future release is not dependent on this document alone.

The concrete April 2026 artifact evidence is recorded in
[`releases/fdc-foundation-2026-04-validation.md`](releases/fdc-foundation-2026-04-validation.md)
and its deterministic JSON report. The report is intentionally blocked: the downloaded JSON has
32 `null` members in the `FoundationFoods` array and no reviewed production selection. No source
record is silently discarded and no catalog activation is authorized by this evidence.
