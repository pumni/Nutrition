# Runtime configuration contract

Status: production-engineering gate

The API and worker require an explicit `APP_ENV` so development-only behavior cannot be enabled by an accidental production deployment.

## Environment classes

| `APP_ENV` | Intended use | Development bearer auth | Fixture parser | Foundation fixture seed |
|---|---|---:|---:|---:|
| `local` | developer workstation | allowed | allowed | allowed |
| `ci` | automated verification | allowed | allowed | allowed |
| `staging` | production-equivalent validation | forbidden | forbidden | forbidden |
| `production` | real production traffic | forbidden | forbidden | forbidden |

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

`AUTH_MODE=oidc` is reserved for the production adapter tracked by #10. Until that adapter exists, selecting `oidc` fails startup. This deliberately means staging and production API startup remain blocked rather than silently using development credentials.

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

## Secrets and logging

`DATABASE_URL`, `LLM_API_KEY`, authorization tokens, and meal text are sensitive. Do not commit them, bake them into images, or emit them through logging/telemetry. Supply production secrets through the deployment platform's secret mechanism.

The checked-in `.env.example` contains local-only placeholder values and no real credentials.

## Production blockers that remain intentional

This contract makes unsafe configuration fail closed; it does not claim production readiness. In particular:

- production API authentication remains blocked until #10 implements OIDC;
- fixture catalog data remains prohibited in staging/production;
- hosted parser production enablement remains gated by #8, #9, and privacy/legal review;
- a production nutrition source remains gated by #5–#7.
