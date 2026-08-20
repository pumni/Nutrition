# Catalog activation

## Starting state
FDC imports are staged and catalog activation is explicit.

## User task
Improve or repair the staged importer success path.

## Expected behavioral outcome
Import validation and provenance complete successfully without implicitly activating a catalog release.

## Must not do
Do not convert importer success into production activation or mutate released evidence.

## Verification
`cargo xtask fdc`, activation integration tests, and database immutability checks.

## Human-decision expectation
none
