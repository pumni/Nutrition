# Foundation 0.3.0

Status: verified implementation increment  
Previous release: `foundation-0.2.0`

## Delivered

- Separate food-evidence and portion-evidence application ports.
- Fixture grammar broadened to `<quantity> <unit> <food>`.
- Explicit grams remain a direct, observation-free mass resolution path.
- PostgreSQL contextual portion resolver scoped to the active catalog release.
- Test-only `quả`/boiled-egg and `bát`/white-rice observations with mass bounds.
- Deterministic scaling and nutrient-bound propagation for contextual quantities.
- Lower and upper mass persisted in relational analysis items and immutable snapshots.
- Immutable membership for published catalog names, profiles, and portion observations.
- Immutable food names, portion observations, and nutrient values referenced by published data.
- Immutable published catalog release content with an explicit active-to-superseded transition.
- Contextual persistence/replay, rejection-without-persistence, HTTP smoke, and SQL immutability
  coverage.

## Evidence boundary

This release does not introduce production portion evidence. Both observations are synthetic,
quality-C engineering fixtures. A unit is usable only for a matching food observation in the
active catalog release. Unsupported pairs return insufficient evidence; the parser and resolver do
not invent grams.

## Version vector

- Application: `0.3.0`.
- Parser schema: `parsed-meal-0.1.0` (unchanged wire shape).
- Fixture parser behavior: `fixture-parser-0.2.0`.
- Portion policy: `portion-contextual-0.2.0`.
- Fixture catalog: `catalog-foundation-0.2.0`.

## Verification

- Workspace formatting, Clippy with warnings denied, and unit tests.
- Seven SQLx migrations applied.
- Seed applied repeatedly against an existing database.
- PostgreSQL contextual integration and immutable snapshot replay.
- HTTP create/read with item mass bounds.
- Published catalog release and membership mutation rejection.

## Still deferred

- Natural Vietnamese parser and hosted LLM adapter.
- Production source release and real portion measurement study.
- Volume/density, branded serving, and curated-default resolution.
- Recipe calculation.
- Clarification and correction state transitions.
- Authentication, authorization, and idempotency HTTP middleware.
