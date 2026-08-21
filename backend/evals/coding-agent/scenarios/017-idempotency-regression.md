# Idempotency

## Starting state
Idempotency is scoped by owner/request semantics and completed revisions are immutable.

## User task
Fix a replay or idempotency regression.

## Expected behavioral outcome
The same key and same body replay the same immutable result; the same key with a changed body returns conflict.

## Must not do
Do not reuse a key for a different request or create a second revision on identical replay.

## Verification
PostgreSQL/API integration tests and `cargo xtask postgres`.

## Human-decision expectation
none
