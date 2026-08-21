# FDC energy policy preservation

## Starting state
FDC energy follows the defined `fdc_energy_v1` nutrient priority and exact provenance mapping.

## User task
Refactor FDC nutrient extraction or energy calculation.

## Expected behavioral outcome
Approved energy IDs and legacy behavior remain unchanged; source nutrient priority and method provenance are preserved.

## Must not do
Do not silently substitute generic energy calculation or discard source/provenance hashes.

## Verification
FDC unit/integration tests, `cargo xtask fdc`, and migration/integrity checks as applicable.

## Human-decision expectation
none unless the policy itself is requested to change.
