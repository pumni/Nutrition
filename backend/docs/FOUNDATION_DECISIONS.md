# Foundation decisions

Status: implementation baseline  
Behavior release: `foundation-0.4.0`

## Scope

This foundation implements the first deterministic vertical slice:

```text
quantity + unit + exact curated alias
→ explicit grams or contextual portion observation + direct composition profile
→ deterministic calculation
→ transactional relational results + immutable JSON snapshot
→ SHA-256 verified read/replay
```

The fixture parser is intentionally constrained to:

```text
<quantity> <unit> <food>, <quantity> <unit> <food>
```

It is a local/test adapter, not the production Vietnamese parser.

Food identity, exact-name retrieval, profile selection, nutrient evidence, catalog release
pinning, portion lookup, and analysis persistence now use PostgreSQL. The parser remains a fixture
adapter, and the active catalog data remains explicitly test-only.

Food resolution and portion resolution are separate application ports. Explicit grams do not
require a portion observation. Other units require a food-specific observation in the active
catalog release; unsupported pairs produce insufficient evidence rather than a guessed mass.
Observed lower and upper masses are scaled by quantity, propagated by the pure calculator, stored
in relational item rows, and retained in the immutable result snapshot.

For one resolved food with an unsupported portion, the balanced foundation policy asks one
versioned portion question. Answering it creates a new completed revision without calling the
parser. Corrections currently support portion quantity/unit changes by item index; unchanged items
retain their parsed quantity/unit context and are recalculated under the pinned behavior vector.

## Numeric policy

- Domain arithmetic uses `rust_decimal::Decimal`.
- PostgreSQL persistence uses `numeric`.
- Calculation does not round intermediate values.
- Presentation rounding remains outside the domain calculator.
- Reconsider only with benchmark evidence and a calculator behavior release.

## Dependency policy

```text
domain <- application <- adapters
                      <- persistence-postgres
                      <- api-http / worker
```

The domain crate must not import Axum, SQLx, Tokio, provider SDKs, clocks, or random generators.

## Published immutability

Database triggers protect published recipes and composition profiles, their nutrient values, and
released food-name/portion evidence.
Completed analysis revisions are finalized from a temporary `building` state and cannot be changed
afterward. The application layer must also treat them as append-only.

Catalog name, profile, and portion-observation memberships are populated while a release is staged.
After activation or supersession, both the release contents and its memberships are protected by
database triggers. The only allowed mutation of an active release is the lifecycle transition to
`superseded`.

The persistence adapter writes analysis, revision, items, nutrient results, totals, snapshot, and
outbox event in one transaction. A revision starts as `building`; the finalization update supplies
the snapshot and hash before changing it to `completed`.

## Idempotency scope

Analyses without an idempotency key may be created repeatedly. HTTP create keys are scoped to the
anonymous create endpoint; correction keys additionally include the analysis ID. The request hash
and immutable response revision are stored in the same transaction as the workflow write. A key
with the same body replays that revision; a different body returns an idempotency conflict.

## Behavior version vector

Every persisted revision has independent versions for:

- application;
- parser schema;
- prompt;
- provider/model;
- normalization;
- resolution;
- portion;
- composition selection;
- clarification;
- correction;
- calculator;
- catalog release.

No replay path may depend on unrecorded “current” configuration.

## Privacy boundary

The API and telemetry do not log raw meal text. Persistence provides an encrypted raw-text field,
but key management and retention are intentionally not implemented until the product/legal policy
is approved. Item source spans remain sensitive analysis data and must follow the same deletion
policy.

## Deferred

- Hosted LLM provider.
- Production household/count/volume portion measurement study and policy.
- Recipe calculation.
- Production source adapter and curated seed release.
- Authentication provider and curation UI.
- Redis, message broker, vector search, graph database, and Kubernetes.
