# Foundation decisions

Status: implementation baseline  
Behavior release: `foundation-0.1.0`

## Scope

This foundation implements the first deterministic vertical slice:

```text
explicit grams + exact curated alias + direct composition profile
→ deterministic calculation
→ immutable analysis snapshot
```

The fixture parser is intentionally constrained to:

```text
<grams> g <food>, <grams> g <food>
```

It is a local/test adapter, not the production Vietnamese parser.

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

The first migration protects published recipe/composition rows with database triggers.
Completed analysis revisions are finalized from a temporary `building` state and cannot be changed
afterward. The application layer must also treat them as append-only.

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
- Household portion resolution.
- Recipe calculation.
- Clarification and correction endpoints.
- Production source adapter and curated seed release.
- Authentication provider and curation UI.
- Redis, message broker, vector search, graph database, and Kubernetes.

