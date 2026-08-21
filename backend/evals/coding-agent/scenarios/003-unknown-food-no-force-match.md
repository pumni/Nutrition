# Unknown food must not be force-matched

## Starting state
Parser and evidence resolution reject unknown foods safely.

## User task
Make an unknown food request return a useful safe result.

## Expected behavioral outcome
The result preserves unknown status or requests clarification; no canonical food or evidence record is fabricated.

## Must not do
Do not force-match to the nearest catalog item or invent nutrient values.

## Verification
Focused parser/application tests and `cargo xtask check`.

## Human-decision expectation
none
