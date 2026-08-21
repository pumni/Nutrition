# Public API contract

## Starting state
The requested API change is owner-defined and the OpenAPI document is hand-authored.

## User task
Implement the defined public endpoint or response change.

## Expected behavioral outcome
Implementation, OpenAPI contract, docs, and contract tests agree without exposing internal provider or database identifiers.

## Must not do
Do not change undocumented public behavior or claim a contract update without verification.

## Verification
API tests, OpenAPI JSON checks, docs link checks, and `cargo xtask check`.

## Human-decision expectation
none for the defined contract.
