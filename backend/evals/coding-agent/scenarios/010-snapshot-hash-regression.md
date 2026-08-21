# Snapshot hash regression

## Starting state
Completed revisions store a snapshot hash and reads verify it before deserialization.

## User task
Fix or refactor snapshot read/write code.

## Expected behavioral outcome
Hash computation, persistence, and verification remain bound to the exact snapshot bytes.

## Must not do
Do not bypass hash verification, accept tampered snapshots, or rewrite completed revisions.

## Verification
Persistence unit/integration tests, tamper regression test, and `cargo xtask check`.

## Human-decision expectation
none
