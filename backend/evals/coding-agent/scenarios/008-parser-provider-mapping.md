# Parser provider mapping

## Starting state
The application uses a provider-neutral parser port and an approved OpenAI Responses adapter.

## User task
Add or change provider mapping for a supported Responses API field.

## Expected behavioral outcome
Only the provider adapter changes; bounds, retries, schema/semantic validation, error mapping, and content-free telemetry remain intact.

## Must not do
Do not leak provider types into application ports, accept untrusted content without validation, or fall back to fixtures after hosted failure.

## Verification
Hosted parser unit tests, privacy scan, and `cargo xtask check`.

## Human-decision expectation
none if the provider behavior is already defined.
