# FDC Foundation Foods April 2026 validation evidence

The validation report in `fdc-foundation-2026-04-validation.json` was generated from the
official USDA Foundation Foods April 2026 JSON download on 2026-08-15. The download page is
`https://fdc.nal.usda.gov/download-datasets/` and the pinned archive URI is recorded in the
report.

## Evidence

- Archive SHA-256: `186e988ec542e913f51ef62b86a47758e8cdd0d1dc3889e7b055581f3c09c77a`
- Extracted JSON filename: `FoodData_Central_foundation_food_json_2026-04-30.json`
- Extracted JSON SHA-256: `27d1fe3fd89edfbe528ed915da5619320e1d004d4594603a1b19bdb1511590cc`
- Importer: `fdc-foundation-json-0.2.0`
- Energy policy: `fdc_energy_v1`
- Source array entries: `395`
- Structurally valid food records: `363`
- Structural anomalies: `32` trailing `null` entries at indexes `363..394`
- Reviewed production selection: not approved; selected records `0`
- Activation attempted: no
- Production eligible: no

The source-wide energy summary is `2048 = 199`, `2047 fallback = 27`, missing energy `= 137`,
and unexpected legacy `1008 = 95`. These counts are evidence about the valid source records;
they do not approve any food subset for catalog staging.

## Protected-decision report

- Classification: source artifact/schema validation gate.
- Evidence: the exact downloaded JSON contains 32 `null` members in the `FoundationFoods`
  array. The importer correctly fails closed because its contract requires each array member to
  be a Foundation food object.
- Constraint: this PR must not silently discard records, alter the pinned bytes, or change the
  source schema contract without an explicit versioned source decision.
- Impact: artifact and checksum evidence are captured, but #6 cannot treat this release as
  validation-passed and no catalog activation/publication is possible.
- Smallest human decision: confirm either (a) a corrected official artifact/release, or (b) an
  explicitly versioned preprocessing rule that removes only these null placeholders while
  retaining the original archive hash and an auditable transformed-payload hash. The latter
  would be a new source adapter contract and must not be inferred by the agent.
