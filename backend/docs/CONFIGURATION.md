# Runtime configuration contract

Status: production-engineering gate

The API and worker require an explicit `APP_ENV` so development-only behavior cannot be enabled by an accidental production deployment.

## Environment classes

| `APP_ENV` | Intended use | Development bearer auth | Fixture parser | Foundation fixture seed | Staged source import |
|---|---|---:|---:|---:|---:|
| `local` | developer workstation | allowed | allowed | allowed | allowed |
| `ci` | automated verification | allowed | allowed | allowed | allowed |
| `staging` | production-equivalent validation | forbidden | forbidden | forbidden | allowed |
| `production` | real production traffic | forbidden | forbidden | forbidden | forbidden |

`APP_ENV` is required. Unknown values fail startup.

## API configuration

Required in every environment:

- `APP_ENV`
- `DATABASE_URL`
- `AUTH_MODE`
- `PARSER_MODE`

`APP_BIND_ADDR` may be omitted only for `local` and `ci`, where it defaults to `127.0.0.1:8080`. Staging and production must set it explicitly. The production API container defaults it to `0.0.0.0:8080`.

### Authentication

`AUTH_MODE=development` is accepted only for `APP_ENV=local|ci` and uses the existing `Bearer dev:<uuid>` contract.

`AUTH_MODE=oidc` is the provider-neutral OIDC adapter for `staging` and `production`. It requires:

- `OIDC_ISSUER_URL`, an HTTPS issuer URL without a query or fragment;
- `OIDC_AUDIENCE`, the exact expected audience.

The adapter discovers JWKS from the configured issuer, accepts `RS256` only, requires exact `iss`, expected `aud`, non-empty `sub`, and `exp`, and validates optional `nbf` with a 60-second clock-skew allowance. JWKS are cached for 15 minutes; an unknown key ID triggers one refresh and failures remain fail-closed. No provider-specific role or scope authorization is enabled in v1.

### Parser

`PARSER_MODE=fixture` is accepted only for `APP_ENV=local|ci`.

`PARSER_MODE=hosted` requires:

- `LLM_ENDPOINT`
- `LLM_API_KEY`
- `LLM_PROVIDER`
- `LLM_MODEL`

Optional bounded hosted-parser settings remain:

- `LLM_TIMEOUT_MS` (default `3000`)
- `LLM_MAXIMUM_RESPONSE_BYTES` (default `65536`)
- `LLM_CIRCUIT_FAILURE_THRESHOLD` (default `5`)
- `LLM_CIRCUIT_COOLDOWN_SECONDS` (default `30`)

Hosted mode being syntactically valid does not waive the benchmark, privacy, legal, residency, retention, or operational gates in `docs/HOSTED_PARSER.md`.

## Worker configuration

Required in every environment:

- `APP_ENV`
- `DATABASE_URL`

For `staging` and `production`, `WORKER_MODE` and `WORKER_ID` are also required. `WORKER_MODE` must be one of `idle`, `run-once`, or `loop`.

For `local` and `ci`, an omitted `WORKER_MODE` defaults to `idle` and an omitted `WORKER_ID` defaults to `worker-local`.

Optional bounded settings:

- `WORKER_DATABASE_POOL_SIZE` (default `4`, range `1..=32`)
- `WORKER_BATCH_SIZE` (default `20`, range `1..=100`)
- `WORKER_POLL_INTERVAL_MS` (default `500`, range `50..=60000`)

`RUN_MIGRATIONS=true` explicitly runs the embedded forward-only migrations. `RUN_FOUNDATION_SEED=true` is accepted only for `local` and `ci`; staging and production reject it before fixture data can be inserted.

## Staged FoodData Central import

`RUN_FDC_FOUNDATION_IMPORT=true` executes the release-pinned USDA FoodData Central Foundation Foods importer during worker startup. It is allowed only for `local`, `ci`, and `staging`; production rejects it. The importer writes raw provenance and a staged catalog selection only. It never activates a catalog release or publishes a composition profile.

When enabled, all of the following are required:

- `FDC_IMPORT_PATH`: local path to the extracted Foundation Foods JSON artifact available to the worker process;
- `FDC_IMPORT_RELEASE_VERSION`: pinned upstream release identifier, initially `2026-04-30`;
- `FDC_IMPORT_SOURCE_PUBLISHED_DATE`: upstream publication date, initially `2026-04-30`;
- `FDC_IMPORT_OBJECT_URI`: durable provenance URI for the exact imported JSON artifact;
- `FDC_IMPORT_EXPECTED_SHA256`: expected SHA-256 of the exact JSON bytes read from `FDC_IMPORT_PATH`;
- `FDC_IMPORT_SOURCE_ARCHIVE_SHA256`: optional SHA-256 of the source archive; required when a
  preprocessing policy is enabled;
- `FDC_IMPORT_PREPROCESSING_POLICY`: optional exact adapter version, currently
  `fdc_foundation_2026_04_null_tail_v1`; the importer verifies the source release/archive/payload
  hashes before consuming its 363-record derivative;
- `FDC_IMPORT_INCLUDE_IDS`: comma-separated, non-empty set of reviewed FDC IDs to stage into the product catalog;
- `FDC_IMPORT_CREATED_BY`: UUID of the human/service actor responsible for the staged import.

The pinned artifact checksum is verified before any database write. If preprocessing is enabled,
the exact source and policy are verified before the derivative is parsed; the original source hash
and derivative hash/policy are retained in the staged manifest. All source records in the artifact
are stored in `raw.source_food_record`, while only the reviewed `FDC_IMPORT_INCLUDE_IDS` selection
receives staged food/name/profile evidence. Re-importing the same release/checksum/selection is
idempotent. A checksum conflict for the same upstream release fails closed.

The importer stages unambiguous macronutrients plus Foundation energy under `fdc_energy_v1`: nutrient `2048` (Atwater Specific) is preferred and `2047` (Atwater General) is the fallback. Nutrient `1008` is never used for the April 2026 Foundation release. Profiles with missing energy remain incomplete; malformed or duplicate energy candidates fail closed. Imported values retain source nutrient ID/method metadata, while profiles remain `in_review`, quality `U`, and non-production-eligible until the validation/reviewer/activation gates are complete.

Do not place source artifacts or real credentials in the repository. The import file should be supplied by the controlled data-ingestion environment.

## Secrets and logging

`DATABASE_URL`, `LLM_API_KEY`, authorization tokens, and meal text are sensitive. Do not commit them, bake them into images, or emit them through logging/telemetry. Supply production secrets through the deployment platform's secret mechanism.

The checked-in `.env.example` contains local-only placeholder values and no real credentials.

## Production blockers that remain intentional

This contract makes unsafe configuration fail closed; it does not claim production readiness. In particular:

- production OIDC traffic remains a deployment/provider approval gate; the checked-in adapter does not select or enable a provider by itself;
- fixture catalog data remains prohibited in staging/production;
- hosted parser production enablement remains gated by #8, #9, and privacy/legal review;
- FDC source staging remains non-publishing until the `fdc_energy_v1` validation and source/reviewer gates from #5–#6 are resolved;
- Vietnamese source rights and portion evidence remain gated by #5 and #7.
