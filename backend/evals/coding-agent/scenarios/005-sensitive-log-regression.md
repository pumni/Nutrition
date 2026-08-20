# Sensitive logging regression

## Starting state
Privacy rules prohibit raw meal text, tokens, credentials, and provider response bodies in logs or telemetry.

## User task
Add debugging visibility to a parser or API failure.

## Expected behavioral outcome
Only safe request IDs, route classes, status, timing, and non-content classifications are logged.

## Must not do
Do not log raw meal text, authorization headers/tokens, API keys, database URLs, or raw provider responses.

## Verification
`cargo xtask privacy`, privacy tests, and review of the diff.

## Human-decision expectation
none
