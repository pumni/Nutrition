# FDC Foundation Foods April 2026 validation evidence

The validation report in `fdc-foundation-2026-04-validation.json` was generated from the
official USDA Foundation Foods April 2026 JSON download on 2026-08-15. The download page is
`https://fdc.nal.usda.gov/download-datasets/` and the pinned archive URI is recorded in the
report.

## Evidence

- Archive SHA-256: `186e988ec542e913f51ef62b86a47758e8cdd0d1dc3889e7b055581f3c09c77a`
- Extracted JSON filename: `FoodData_Central_foundation_food_json_2026-04-30.json`
- Extracted JSON SHA-256: `27d1fe3fd89edfbe528ed915da5619320e1d004d4594603a1b19bdb1511590cc`
- Preprocessing policy: `fdc_foundation_2026_04_null_tail_v1`
- Transformed payload SHA-256: `8af923182f75bce502ba9c14aca2228fd4dad095eb1a8d6a7aba6a2b5101c19d`
- Importer: `fdc-foundation-json-0.2.0`
- Energy policy: `fdc_energy_v1`
- Source array entries: `395`
- Structurally valid food records: `363`
- Structural anomalies: `32` trailing `null` entries at indexes `363..394`
- Preprocessing: removed exactly those `32` tail placeholders; normalized payload contains `363` records
- Source integrity valid: yes; source schema conformant: no; normalized payload valid: yes
- Reviewed production selection: not approved; selected records `0`
- Activation attempted: no
- Production eligible: no

The source-wide energy summary is `2048 = 199`, `2047 fallback = 27`, missing energy `= 137`,
and unexpected legacy `1008 = 95`. These counts are evidence about the valid source records;
they do not approve any food subset for catalog staging.

## Versioned preprocessing decision

- Classification: release-specific source adapter.
- Approved policy: `fdc_foundation_2026_04_null_tail_v1` applies only to the exact archive and
  extracted JSON hashes recorded above. It requires `FoundationFoods.length == 395`, valid
  non-null entries at indexes `0..362`, and exactly `null` entries at indexes `363..394`.
- Transform: remove only indexes `363..394`; do not reorder, normalize, synthesize, or otherwise
  modify any food record. The original archive and extracted JSON remain the source evidence;
  the transformed payload is a separate derivative identified by its own SHA-256.
- Fail-closed conditions: any source hash, release, length, null position, tail value, or policy
  mismatch rejects preprocessing.
- Production gate: still blocked. The normalized artifact has no reviewed FDC selection and no
  named reviewer approval. The `137` records without `2047`/`2048` remain incomplete, and
  legacy `1008` is not used as a fallback.
