# Foundation decisions

Status: implementation baseline  
Behavior release: `foundation-0.6.0`

## Scope

This foundation implements the first deterministic vertical slice:

```text
quantity + unit + exact curated alias
→ explicit grams or contextual portion observation + direct composition profile
→ deterministic calculation
→ transactional relational results + immutable JSON snapshot
→ SHA-256 verified read/replay
```

The fixture parser remains intentionally constrained to:

```text
<quantity> <unit> <food>, <quantity> <unit> <food>
```

It is a local/test adapter, not the production Vietnamese parser. Runtime selection is explicit:
`PARSER_MODE=fixture|hosted`; absence or an unknown value fails startup, and hosted failure never
falls back to the fixture.

Food identity, exact-name retrieval, profile selection, nutrient evidence, catalog release
pinning, portion lookup, and analysis persistence use PostgreSQL. Hosted parsing is available
behind the existing application port, while active catalog data remains explicitly test-only.

## Hosted parser boundary

The provider adapter accepts only an HTTPS endpoint and bounded configuration. It sends a fixed
system instruction, the strict versioned JSON schema, locale, and untrusted meal text. It does not
send user identity, authorization, history, nutrition data, internal IDs, or source URLs.

Provider output is untrusted. The adapter applies a byte limit while streaming the response,
strict envelope deserialization, JSON Schema validation, then semantic checks that require every
source span and food phrase to occur in the input and reject negated consumption or duplicates.
Schema failure and transient transport/timeout failure receive at most one retry. Semantic failure
does not retry. Repeated failures open a provider/model circuit and all terminal failures surface
as parser unavailable rather than fabricated analysis.

Hosted invocation telemetry is best-effort so an observability outage does not alter nutrition
behavior. PostgreSQL stores provider/model, prompt/schema versions, latency, one-bit retry count,
optional token counts, output SHA-256, status, and a bounded error code. It has no raw request,
response, meal text, user, or authorization column.

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

HTTP create, clarification, and correction requests require an idempotency key. Keys are scoped to the
authenticated principal and operation; clarification and correction keys additionally include the
analysis ID. The versioned typed request hash and immutable response revision are stored in the same
transaction as the workflow write. A key with the same body replays that revision; a different body
returns an idempotency conflict.

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

The API and telemetry do not log raw meal text. Hosted telemetry stores only an output hash and
non-content operational metadata. Persistence provides an encrypted raw-text field,
but key management and retention are intentionally not implemented until the product/legal policy
is approved. Item source spans remain sensitive analysis data and must follow the same deletion
policy.

API analysis routes require an authenticated principal and enforce PostgreSQL ownership
before read, clarification, correction, or history access. The development bearer format is not a
production authentication mechanism. Non-development authentication uses the configured provider-
neutral OIDC adapter; provider selection and production deployment remain explicit gates. Request
bodies are capped at 16 KiB, and verification scans logging
macros for meal text, authorization, raw text, and database URL usage.

## Worker reliability boundary

Jobs are claimed with `FOR UPDATE SKIP LOCKED`, increment attempts at claim, and move through
bounded retry or `dead`. Running jobs require a lease owner and timestamp. The worker supports
typed `idle`, `run-once`, and continuous loop modes with bounded batches and graceful shutdown.
Outbox run-once delivery currently targets a database-local test sink by setting `published_at`;
an external transport adapter remains deferred.

## Deferred

- Provider-specific hosted LLM contract mapping, legal approval, data residency, and retention.
- Staging parser evaluation against the approved Vietnamese meal benchmark.
- Production household/count/volume portion measurement study and policy.
- Recipe calculation.
- Production source adapter and curated seed release.
- Production OIDC provider and curation UI.
- Redis, message broker, vector search, graph database, and Kubernetes.
